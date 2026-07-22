// Format / align: re-columnize each line into label · mnemonic · operands ·
// comment. Column alignment only — it never rewrites operand internals (safe
// for strings and expressions). String-aware comment splitting via the
// tokenizer, so `DB "a;b"` is never mistaken for a comment.

import { tokenize } from "./highlight.js";

const MNEMONIC_COL = 8;
const COMMENT_COL = 32;

function padTo(s: string, col: number, minGap: number): string {
  const gap = Math.max(col - s.length, minGap);
  return s + " ".repeat(gap);
}

/** Format a single source line. */
export function formatLine(line: string): string {
  // Split off a trailing comment (string-aware).
  const toks = tokenize(line);
  const ci = toks.findIndex((t) => t.kind === "comment");
  let comment = "";
  let codeEnd = line.length;
  if (ci >= 0) {
    comment = toks[ci].text.trim();
    codeEnd = toks.slice(0, ci).reduce((a, t) => a + t.text.length, 0);
  }
  let code = line.slice(0, codeEnd).trim();

  // A full-line comment (or blank line) — keep at column 0.
  if (code === "") return comment;

  // Leading label (`name:`), if any.
  let label = "";
  const lm = code.match(/^([A-Za-z_.][\w.]*:)/);
  if (lm) {
    label = lm[1];
    code = code.slice(lm[1].length).trim();
  }

  // Mnemonic + operands (operands kept verbatim, only trimmed).
  let body = "";
  if (code) {
    const bm = code.match(/^(\S+)(?:\s+([\s\S]*))?$/);
    const mnem = bm ? bm[1] : code;
    const ops = bm && bm[2] ? bm[2].trim() : "";
    body = ops ? `${mnem} ${ops}` : mnem;
  }

  let out = label;
  if (body) out = padTo(out, MNEMONIC_COL, out === "" ? MNEMONIC_COL : 1) + body;
  if (comment) out = out === "" ? comment : padTo(out, COMMENT_COL, 2) + comment;
  return out.trimEnd();
}

/** Format an entire source document. Idempotent. */
export function formatSource(source: string): string {
  return source.split("\n").map(formatLine).join("\n");
}
