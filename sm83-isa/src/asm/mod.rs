//! Two-pass assembler: labels, EQU constants, ORG/DB/DW/DS directives,
//! and full expressions over the single-instruction parser/encoder.

pub mod assemble;
pub mod expr;

pub use assemble::{assemble, Assembled, Diagnostic};
pub use expr::{eval, parse_expr, Expr};
