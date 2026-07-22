// Inlay hints: show the resolved value of a symbol *reference* inline, e.g.
// `LD HL, SCREEN ‹$9800›`. Skips the definition site (a label `foo:` is a label
// token, and the definiendum of `NAME EQU …` / `NAME = …` is excluded).

import { tokenize } from "./highlight.js";

export interface InlayHint {
  /** 1-based source line. */
  line: number;
  /** Character column just after the referenced token (for inline placement). */
  col: number;
  symbol: string;
  value: number;
}

export function inlayHints(source: string, symbols: Record<string, number>): InlayHint[] {
  const hints: InlayHint[] = [];
  const lines = source.split("\n");
  const has = (k: string) => Object.prototype.hasOwnProperty.call(symbols, k);

  for (let li = 0; li < lines.length; li++) {
    const toks = tokenize(lines[li]);

    // A definition line — `NAME EQU …` or `NAME = …` — must not hint its own name.
    let defName: string | null = null;
    const sig = toks.filter((t) => t.kind !== "ws");
    if (sig.length >= 2 && sig[0].kind === "ident") {
      const t1 = sig[1];
      const isEqu =
        (t1.kind === "directive" && t1.text.toUpperCase() === "EQU") ||
        (t1.kind === "punct" && t1.text === "=");
      if (isEqu) defName = sig[0].text;
    }

    let col = 0;
    for (const t of toks) {
      col += t.text.length;
      if (t.kind === "ident" && t.text !== defName && has(t.text)) {
        hints.push({ line: li + 1, col, symbol: t.text, value: symbols[t.text] });
      }
    }
  }
  return hints;
}

/** `$XXXX` for address-scale values, `$XX` for small constants. */
export function formatHintValue(v: number): string {
  const u = v & 0xffff;
  return "$" + u.toString(16).toUpperCase().padStart(u > 0xff ? 4 : 2, "0");
}
