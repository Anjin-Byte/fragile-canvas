//! Assembler expressions: Pratt parser + evaluator.
//!
//! Grammar: numbers (`$`/`0x`/`%`/decimal), symbols, `@` (current
//! address), unary `- + ~`, binary `| ^ & << >> + - * / %`, parens, and
//! `HIGH(x)` / `LOW(x)`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i64),
    Sym(String),
    /// `@` — address of the current instruction/directive.
    CurAddr,
    Unary(UnaryOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    High(Box<Expr>),
    Low(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    Xor,
    And,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::Xor => 2,
        BinOp::And => 3,
        BinOp::Shl | BinOp::Shr => 4,
        BinOp::Add | BinOp::Sub => 5,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
    }
}

pub struct ExprError(pub String);

struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn skip_ws(&mut self) {
        while self.src[self.pos..].starts_with(|c: char| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.src[self.pos..].chars().next()
    }

    fn eat(&mut self, s: &str) -> bool {
        self.skip_ws();
        if self.src[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn take_while(&mut self, pred: impl Fn(char) -> bool) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.src[self.pos..].chars().next() {
            if pred(c) {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        &self.src[start..self.pos]
    }
}

/// Parse a complete expression; the whole input must be consumed.
pub fn parse_expr(src: &str) -> Result<Expr, ExprError> {
    let mut sc = Scanner { src, pos: 0 };
    let e = parse_prec(&mut sc, 0)?;
    sc.skip_ws();
    if sc.pos != src.len() {
        return Err(ExprError(format!("unexpected input at '{}'", &src[sc.pos..])));
    }
    Ok(e)
}

fn parse_prec(sc: &mut Scanner, min_prec: u8) -> Result<Expr, ExprError> {
    let mut lhs = parse_atom(sc)?;

    loop {
        sc.skip_ws();
        let rest = &sc.src[sc.pos..];
        let (op, len) = if rest.starts_with("<<") {
            (BinOp::Shl, 2)
        } else if rest.starts_with(">>") {
            (BinOp::Shr, 2)
        } else {
            match rest.chars().next() {
                Some('|') => (BinOp::Or, 1),
                Some('^') => (BinOp::Xor, 1),
                Some('&') => (BinOp::And, 1),
                Some('+') => (BinOp::Add, 1),
                Some('-') => (BinOp::Sub, 1),
                Some('*') => (BinOp::Mul, 1),
                Some('/') => (BinOp::Div, 1),
                Some('%') => (BinOp::Rem, 1),
                _ => break,
            }
        };
        // `%` is also the binary-literal prefix; treat as operator only
        // when followed by something that isn't a binary digit run that
        // could start an atom — operator context always follows a
        // complete lhs, so plain '%' here is division remainder.
        let prec = precedence(op);
        if prec < min_prec {
            break;
        }
        sc.pos += len;
        let rhs = parse_prec(sc, prec + 1)?;
        lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_atom(sc: &mut Scanner) -> Result<Expr, ExprError> {
    match sc.peek() {
        None => Err(ExprError("expected expression".into())),
        Some('(') => {
            sc.eat("(");
            let e = parse_prec(sc, 0)?;
            if !sc.eat(")") {
                return Err(ExprError("missing ')'".into()));
            }
            Ok(e)
        }
        Some('-') => {
            sc.eat("-");
            Ok(Expr::Unary(UnaryOp::Neg, Box::new(parse_atom(sc)?)))
        }
        Some('~') => {
            sc.eat("~");
            Ok(Expr::Unary(UnaryOp::Not, Box::new(parse_atom(sc)?)))
        }
        Some('+') => {
            sc.eat("+");
            parse_atom(sc)
        }
        Some('@') => {
            sc.eat("@");
            Ok(Expr::CurAddr)
        }
        Some('$') => {
            sc.eat("$");
            let digits = sc.take_while(|c| c.is_ascii_hexdigit());
            if digits.is_empty() {
                return Err(ExprError("'$' needs hex digits".into()));
            }
            i64::from_str_radix(digits, 16)
                .map(Expr::Num)
                .map_err(|e| ExprError(format!("bad hex: {e}")))
        }
        Some('%') => {
            sc.eat("%");
            let digits = sc.take_while(|c| c == '0' || c == '1');
            if digits.is_empty() {
                return Err(ExprError("'%' needs binary digits".into()));
            }
            i64::from_str_radix(digits, 2)
                .map(Expr::Num)
                .map_err(|e| ExprError(format!("bad binary: {e}")))
        }
        Some(c) if c.is_ascii_digit() => {
            // 0x prefix or decimal
            let text = sc.take_while(|c| c.is_ascii_alphanumeric() || c == '_');
            if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                i64::from_str_radix(hex, 16)
                    .map(Expr::Num)
                    .map_err(|e| ExprError(format!("bad hex: {e}")))
            } else {
                text.parse::<i64>()
                    .map(Expr::Num)
                    .map_err(|_| ExprError(format!("bad number: {text}")))
            }
        }
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '.' => {
            let ident = sc.take_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
            let upper = ident.to_ascii_uppercase();
            if (upper == "HIGH" || upper == "LOW") && sc.peek() == Some('(') {
                sc.eat("(");
                let inner = parse_prec(sc, 0)?;
                if !sc.eat(")") {
                    return Err(ExprError("missing ')'".into()));
                }
                Ok(if upper == "HIGH" {
                    Expr::High(Box::new(inner))
                } else {
                    Expr::Low(Box::new(inner))
                })
            } else {
                Ok(Expr::Sym(ident.to_string()))
            }
        }
        Some(c) => Err(ExprError(format!("unexpected '{c}'"))),
    }
}

/// Evaluate with a symbol resolver and the current address.
pub fn eval(
    expr: &Expr,
    cur_addr: u16,
    lookup: &dyn Fn(&str) -> Option<i64>,
) -> Result<i64, ExprError> {
    Ok(match expr {
        Expr::Num(v) => *v,
        Expr::CurAddr => cur_addr as i64,
        Expr::Sym(name) => {
            lookup(name).ok_or_else(|| ExprError(format!("undefined symbol '{name}'")))?
        }
        Expr::Unary(UnaryOp::Neg, e) => -eval(e, cur_addr, lookup)?,
        Expr::Unary(UnaryOp::Not, e) => !eval(e, cur_addr, lookup)?,
        Expr::High(e) => (eval(e, cur_addr, lookup)? >> 8) & 0xFF,
        Expr::Low(e) => eval(e, cur_addr, lookup)? & 0xFF,
        Expr::Binary(op, a, b) => {
            let a = eval(a, cur_addr, lookup)?;
            let b = eval(b, cur_addr, lookup)?;
            match op {
                BinOp::Or => a | b,
                BinOp::Xor => a ^ b,
                BinOp::And => a & b,
                BinOp::Shl => a << (b & 63),
                BinOp::Shr => a >> (b & 63),
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => {
                    if b == 0 {
                        return Err(ExprError("division by zero".into()));
                    }
                    a / b
                }
                BinOp::Rem => {
                    if b == 0 {
                        return Err(ExprError("modulo by zero".into()));
                    }
                    a % b
                }
            }
        }
    })
}
