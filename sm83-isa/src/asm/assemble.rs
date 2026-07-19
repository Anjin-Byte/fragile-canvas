//! Two-pass assembler driver.
//!
//! Pass 1 walks the source with a location counter: defines labels/EQUs
//! and sizes every line. Instruction sizes come from encoding the operand
//! *shapes* with dummy values — on SM83, size never depends on operand
//! values, so no relaxation loop is needed. Pass 2 evaluates expressions
//! against the complete symbol table, range-checks, and emits bytes.
//!
//! Errors don't abort: every problem becomes a line-tagged `Diagnostic`
//! and assembly continues, so one run reports everything.

use std::collections::{BTreeMap, HashMap};

use crate::asm::expr::{eval, parse_expr, Expr};
use crate::encode::encode;
use crate::model::{Instruction, Mnemonic, Operand};
use crate::parse::{coerce, mnemonic_from_str, parse_raw_operand, split_operands, RawOperand};

// ─── Diagnostics ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based source line.
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

// ─── Output ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Assembled {
    /// Contiguous byte runs keyed by start address (ORG starts a new one).
    pub segments: BTreeMap<u16, Vec<u8>>,
    /// Label/EQU values after pass 1.
    pub symbols: HashMap<String, i64>,
    /// Address/bytes/source listing (pass-2 view).
    pub listing: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl Assembled {
    pub fn ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Flatten segments into one image starting at the lowest address,
    /// padding gaps with `fill`. Returns `(origin, bytes)`.
    pub fn flatten(&self, fill: u8) -> Option<(u16, Vec<u8>)> {
        let (&origin, _) = self.segments.iter().next()?;
        let end = self
            .segments
            .iter()
            .map(|(&a, b)| a as usize + b.len())
            .max()
            .unwrap_or(origin as usize);
        let mut out = vec![fill; end - origin as usize];
        for (&addr, bytes) in &self.segments {
            let off = (addr - origin) as usize;
            out[off..off + bytes.len()].copy_from_slice(bytes);
        }
        Some((origin, out))
    }
}

// ─── Line AST ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum OpAst {
    /// Fully resolved by the operand grammar (registers, (HL), @±disp…).
    Raw(RawOperand),
    /// Bare expression — value context (immediates, jump targets).
    Expr(Expr),
    /// Parenthesized expression — memory context.
    Mem(Expr),
}

#[derive(Debug, Clone)]
enum DbItem {
    Expr(Expr),
    Str(String),
}

#[derive(Debug, Clone)]
enum Body {
    None,
    Instr { mnemonic: Mnemonic, operands: Vec<OpAst> },
    Org(Expr),
    Db(Vec<DbItem>),
    Dw(Vec<Expr>),
    Ds(Expr, Option<Expr>),
    Equ(String, Expr),
}

#[derive(Debug)]
struct Line {
    no: usize,
    label: Option<String>,
    body: Body,
    text: String,
}

// ─── Source parsing ─────────────────────────────────────────────────────────

/// Strip a `;` comment, honoring double-quoted strings.
fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            ';' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '.')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Split `DB` items on top-level commas, honoring strings.
fn split_db_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut in_str, mut start) = (0i32, false, 0usize);
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '(' | '[' if !in_str => depth += 1,
            ')' | ']' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn parse_operand_ast(s: &str) -> Result<OpAst, String> {
    // The value-level grammar first: registers, conditions, (HL)-family,
    // ($FF00+n), @±disp, SP±n, plain numbers.
    if let Ok(raw) = parse_raw_operand(s) {
        return Ok(OpAst::Raw(raw));
    }
    // Fall back to expressions: `(expr)`/`[expr]` is a memory operand,
    // anything else a value operand.
    let s = s.trim();
    let indirect =
        (s.starts_with('(') && s.ends_with(')')) || (s.starts_with('[') && s.ends_with(']'));
    if indirect {
        let inner = &s[1..s.len() - 1];
        return parse_expr(inner).map(OpAst::Mem).map_err(|e| e.0);
    }
    parse_expr(s).map(OpAst::Expr).map_err(|e| e.0)
}

fn parse_line(no: usize, raw_line: &str) -> Result<Line, String> {
    let text = raw_line.to_string();
    let mut rest = strip_comment(raw_line).trim();

    // Leading label: `ident:`
    let mut label = None;
    if let Some(colon) = rest.find(':') {
        let candidate = rest[..colon].trim();
        if is_ident(candidate) {
            label = Some(candidate.to_string());
            rest = rest[colon + 1..].trim();
        }
    }

    if rest.is_empty() {
        return Ok(Line { no, label, body: Body::None, text });
    }

    let (word, tail) = match rest.find(char::is_whitespace) {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => (rest, ""),
    };
    let upper = word.to_ascii_uppercase();
    let upper = upper.strip_prefix('.').unwrap_or(&upper);

    // `name EQU expr` / `name = expr`
    if label.is_none() && is_ident(word) && mnemonic_from_str(word).is_none() {
        let (next, expr_src) = match tail.find(char::is_whitespace) {
            Some(i) => (&tail[..i], tail[i..].trim()),
            None => (tail, ""),
        };
        let next_up = next.to_ascii_uppercase();
        if next_up == "EQU" || next == "=" {
            let e = parse_expr(expr_src).map_err(|e| e.0)?;
            return Ok(Line { no, label: None, body: Body::Equ(word.to_string(), e), text });
        }
    }

    let body = match upper {
        "ORG" => Body::Org(parse_expr(tail).map_err(|e| e.0)?),
        "DB" | "BYTE" => {
            let items = split_db_items(tail)
                .into_iter()
                .map(|item| {
                    if item.starts_with('"') && item.ends_with('"') && item.len() >= 2 {
                        Ok(DbItem::Str(item[1..item.len() - 1].to_string()))
                    } else {
                        parse_expr(item).map(DbItem::Expr).map_err(|e| e.0)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if items.is_empty() {
                return Err("DB needs at least one item".into());
            }
            Body::Db(items)
        }
        "DW" | "WORD" => {
            let items = split_db_items(tail)
                .into_iter()
                .map(|item| parse_expr(item).map_err(|e| e.0))
                .collect::<Result<Vec<_>, _>>()?;
            if items.is_empty() {
                return Err("DW needs at least one item".into());
            }
            Body::Dw(items)
        }
        "DS" | "SPACE" => {
            let items = split_db_items(tail);
            match items.as_slice() {
                [count] => Body::Ds(parse_expr(count).map_err(|e| e.0)?, None),
                [count, fill] => Body::Ds(
                    parse_expr(count).map_err(|e| e.0)?,
                    Some(parse_expr(fill).map_err(|e| e.0)?),
                ),
                _ => return Err("DS takes a count and an optional fill byte".into()),
            }
        }
        _ => match mnemonic_from_str(word) {
            Some(mnemonic) => {
                let operands = split_operands(tail)
                    .into_iter()
                    .map(parse_operand_ast)
                    .collect::<Result<Vec<_>, _>>()?;
                Body::Instr { mnemonic, operands }
            }
            None => return Err(format!("unknown mnemonic or directive: {word}")),
        },
    };

    Ok(Line { no, label, body, text })
}

// ─── Operand resolution ─────────────────────────────────────────────────────

/// Turn operand ASTs into `RawOperand`s. `values` is `None` in pass 1
/// (dummy zeros — shapes only) or `Some((cur_addr, lookup))` in pass 2.
fn resolve_operands(
    mnemonic: Mnemonic,
    operands: &[OpAst],
    ctx: Option<(u16, &dyn Fn(&str) -> Option<i64>)>,
) -> Result<Vec<RawOperand>, String> {
    operands
        .iter()
        .map(|op| {
            Ok(match op {
                OpAst::Raw(r) => *r,
                OpAst::Expr(e) => {
                    let v = match ctx {
                        Some((addr, lookup)) => eval(e, addr, lookup).map_err(|e| e.0)?,
                        None => 0,
                    };
                    if mnemonic == Mnemonic::Jr {
                        // Absolute target → relative displacement.
                        match ctx {
                            Some((addr, _)) => {
                                let disp = v - (addr as i64 + 2);
                                if !(-128..=127).contains(&disp) {
                                    return Err(format!(
                                        "JR target out of range: {disp} bytes away"
                                    ));
                                }
                                RawOperand::Op(Operand::Rel(disp as i8))
                            }
                            None => RawOperand::Op(Operand::Rel(0)),
                        }
                    } else {
                        if !(-65536..=65535).contains(&v) {
                            return Err(format!("value out of range: {v}"));
                        }
                        RawOperand::Num(v as i32)
                    }
                }
                OpAst::Mem(e) => {
                    let v = match ctx {
                        Some((addr, lookup)) => eval(e, addr, lookup).map_err(|e| e.0)?,
                        None => 0,
                    };
                    if !(0..=0xFFFF).contains(&v) {
                        return Err(format!("address out of range: {v}"));
                    }
                    RawOperand::Mem(v as i32)
                }
            })
        })
        .collect()
}

fn build_instruction(
    mnemonic: Mnemonic,
    operands: &[OpAst],
    ctx: Option<(u16, &dyn Fn(&str) -> Option<i64>)>,
) -> Result<Instruction, String> {
    let raw = resolve_operands(mnemonic, operands, ctx)?;
    coerce(mnemonic, &raw).map_err(|e| e.msg)
}

// ─── The two passes ─────────────────────────────────────────────────────────

pub fn assemble(source: &str) -> Assembled {
    let mut out = Assembled::default();

    // Parse every line first; parse errors don't stop the walk.
    let mut lines = Vec::new();
    for (i, raw_line) in source.lines().enumerate() {
        match parse_line(i + 1, raw_line) {
            Ok(line) => lines.push(line),
            Err(msg) => out.diagnostics.push(Diagnostic { line: i + 1, msg }),
        }
    }

    // ── Pass 1: layout + symbols ──
    let mut symbols: HashMap<String, i64> = HashMap::new();
    let mut cur: i64 = 0;
    let mut addrs: Vec<i64> = Vec::with_capacity(lines.len());
    // Lines whose instruction already failed in pass 1 — don't re-report.
    let mut p1_failed = vec![false; lines.len()];

    for (idx, line) in lines.iter().enumerate() {
        if let Some(label) = &line.label {
            if symbols.contains_key(label) {
                out.diagnostics.push(Diagnostic {
                    line: line.no,
                    msg: format!("duplicate symbol '{label}'"),
                });
            } else {
                symbols.insert(label.clone(), cur);
            }
        }
        addrs.push(cur);

        let lookup = |name: &str| symbols.get(name).copied();
        let size: i64 = match &line.body {
            Body::None => 0,
            Body::Equ(name, expr) => {
                // EQUs evaluate eagerly so ORG/DS can use them in pass 1.
                match eval(expr, cur as u16, &lookup) {
                    Ok(v) => {
                        if symbols.contains_key(name) {
                            out.diagnostics.push(Diagnostic {
                                line: line.no,
                                msg: format!("duplicate symbol '{name}'"),
                            });
                        } else {
                            symbols.insert(name.clone(), v);
                        }
                    }
                    Err(e) => out.diagnostics.push(Diagnostic {
                        line: line.no,
                        msg: format!("EQU: {} (EQUs cannot forward-reference)", e.0),
                    }),
                }
                0
            }
            Body::Org(expr) => {
                match eval(expr, cur as u16, &lookup) {
                    Ok(v) if (0..=0xFFFF).contains(&v) => cur = v,
                    Ok(v) => out.diagnostics.push(Diagnostic {
                        line: line.no,
                        msg: format!("ORG out of range: {v}"),
                    }),
                    Err(e) => out.diagnostics.push(Diagnostic {
                        line: line.no,
                        msg: format!("ORG: {} (must resolve in pass 1)", e.0),
                    }),
                }
                // Label on an ORG line binds to the NEW address.
                if let Some(label) = &line.label {
                    symbols.insert(label.clone(), cur);
                }
                *addrs.last_mut().unwrap() = cur;
                0
            }
            Body::Db(items) => items
                .iter()
                .map(|i| match i {
                    DbItem::Str(s) => s.len() as i64,
                    DbItem::Expr(_) => 1,
                })
                .sum(),
            Body::Dw(items) => 2 * items.len() as i64,
            Body::Ds(count, _) => match eval(count, cur as u16, &lookup) {
                Ok(v) if (0..=0x10000).contains(&v) => v,
                Ok(v) => {
                    out.diagnostics.push(Diagnostic {
                        line: line.no,
                        msg: format!("DS count out of range: {v}"),
                    });
                    0
                }
                Err(e) => {
                    out.diagnostics.push(Diagnostic {
                        line: line.no,
                        msg: format!("DS: {} (must resolve in pass 1)", e.0),
                    });
                    0
                }
            },
            Body::Instr { mnemonic, operands } => {
                match build_instruction(*mnemonic, operands, None)
                    .and_then(|i| encode(&i).map_err(|e| e.0.to_string()))
                {
                    Ok(enc) => enc.len() as i64,
                    Err(msg) => {
                        out.diagnostics.push(Diagnostic { line: line.no, msg });
                        p1_failed[idx] = true;
                        0
                    }
                }
            }
        };

        cur += size;
        if cur > 0x10000 {
            out.diagnostics.push(Diagnostic {
                line: line.no,
                msg: "address counter past $FFFF".into(),
            });
            cur = 0x10000;
        }
    }

    // ── Pass 2: evaluate + emit ──
    let lookup = |name: &str| symbols.get(name).copied();
    let mut listing = String::new();
    let mut seg_start: Option<u16> = None;
    let mut seg_bytes: Vec<u8> = Vec::new();
    let flush = |start: &mut Option<u16>, bytes: &mut Vec<u8>, out: &mut Assembled| {
        if let Some(s) = start.take() {
            if !bytes.is_empty() {
                out.segments.insert(s, std::mem::take(bytes));
            } else {
                bytes.clear();
            }
        }
    };

    for (idx, line) in lines.iter().enumerate() {
        if p1_failed[idx] {
            continue; // already diagnosed in pass 1
        }
        let addr = addrs[idx] as u16;
        let mut bytes: Vec<u8> = Vec::new();
        let emit_err = |msg: String, out: &mut Assembled| {
            out.diagnostics.push(Diagnostic { line: line.no, msg });
        };

        match &line.body {
            Body::None | Body::Equ(..) => {}
            Body::Org(_) => {
                flush(&mut seg_start, &mut seg_bytes, &mut out);
            }
            Body::Db(items) => {
                for item in items {
                    match item {
                        DbItem::Str(s) => bytes.extend_from_slice(s.as_bytes()),
                        DbItem::Expr(e) => match eval(e, addr, &lookup) {
                            Ok(v) if (-128..=255).contains(&v) => bytes.push(v as u8),
                            Ok(v) => emit_err(format!("DB value out of range: {v}"), &mut out),
                            Err(e) => emit_err(e.0, &mut out),
                        },
                    }
                }
            }
            Body::Dw(items) => {
                for e in items {
                    match eval(e, addr, &lookup) {
                        Ok(v) if (0..=0xFFFF).contains(&v) => {
                            bytes.extend_from_slice(&(v as u16).to_le_bytes())
                        }
                        Ok(v) => emit_err(format!("DW value out of range: {v}"), &mut out),
                        Err(e) => emit_err(e.0, &mut out),
                    }
                }
            }
            Body::Ds(count, fill) => {
                let n = eval(count, addr, &lookup).unwrap_or(0).max(0) as usize;
                let f = match fill {
                    Some(e) => match eval(e, addr, &lookup) {
                        Ok(v) if (-128..=255).contains(&v) => v as u8,
                        Ok(v) => {
                            emit_err(format!("DS fill out of range: {v}"), &mut out);
                            0
                        }
                        Err(e) => {
                            emit_err(e.0, &mut out);
                            0
                        }
                    },
                    None => 0,
                };
                bytes.resize(n, f);
            }
            Body::Instr { mnemonic, operands } => {
                match build_instruction(*mnemonic, operands, Some((addr, &lookup)))
                    .and_then(|i| encode(&i).map_err(|e| e.0.to_string()))
                {
                    Ok(enc) => bytes.extend_from_slice(enc.as_slice()),
                    Err(msg) => emit_err(msg, &mut out),
                }
            }
        }

        if !bytes.is_empty() {
            if seg_start.is_none() {
                seg_start = Some(addr);
            }
            let hex: Vec<String> = bytes.iter().take(6).map(|b| format!("{b:02X}")).collect();
            let more = if bytes.len() > 6 { "…" } else { "" };
            listing.push_str(&format!(
                "{addr:04X}  {:<18}  {}\n",
                format!("{}{more}", hex.join(" ")),
                line.text.trim_end()
            ));
            seg_bytes.extend_from_slice(&bytes);
        } else if !line.text.trim().is_empty() {
            listing.push_str(&format!("      {:<18}  {}\n", "", line.text.trim_end()));
        }
    }
    flush(&mut seg_start, &mut seg_bytes, &mut out);

    // Overlap check.
    let mut prev_end: Option<(u16, usize)> = None;
    for (&start, bytes) in &out.segments {
        if let Some((pstart, pend)) = prev_end {
            if (start as usize) < pend {
                out.diagnostics.push(Diagnostic {
                    line: 0,
                    msg: format!(
                        "segment at ${start:04X} overlaps segment starting at ${pstart:04X}"
                    ),
                });
            }
        }
        prev_end = Some((start, start as usize + bytes.len()));
    }

    out.symbols = symbols;
    out.listing = listing;
    out
}
