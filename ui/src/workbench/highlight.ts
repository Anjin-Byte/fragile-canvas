// A small SM83-assembly tokenizer for syntax highlighting. Pure and lossless:
// concatenating every token's `text` reproduces the input line exactly, so the
// editor can render a highlighted mirror layer that lines up with the textarea.

export type TokenKind =
  | "comment"
  | "string"
  | "label"
  | "directive"
  | "mnemonic"
  | "register"
  | "number"
  | "ident"
  | "punct"
  | "ws";

export interface Token {
  text: string;
  kind: TokenKind;
}

// prettier-ignore
const MNEMONICS = new Set([
  "LD", "LDH", "LDI", "LDD", "PUSH", "POP", "ADD", "ADC", "SUB", "SBC", "AND",
  "OR", "XOR", "CP", "INC", "DEC", "SWAP", "DAA", "CPL", "CCF", "SCF", "NOP",
  "HALT", "STOP", "DI", "EI", "RLCA", "RLA", "RRCA", "RRA", "RLC", "RL", "RRC",
  "RR", "SLA", "SRA", "SRL", "BIT", "SET", "RES", "JP", "JR", "CALL", "RET",
  "RETI", "RST",
]);
const DIRECTIVES = new Set(["ORG", "DB", "DW", "DS", "EQU", "HIGH", "LOW"]);
// Registers + conditions share the "register" colour.
const REGISTERS = new Set([
  "A", "B", "C", "D", "E", "H", "L", "AF", "BC", "DE", "HL", "SP", "PC", "NZ", "Z", "NC",
]);

const isWs = (c: string) => c === " " || c === "\t" || c === "\r";
const isDigit = (c: string) => c >= "0" && c <= "9";
const isHex = (c: string) => isDigit(c) || (c >= "a" && c <= "f") || (c >= "A" && c <= "F");
const isIdentStart = (c: string) => /[A-Za-z_.]/.test(c);
const isIdent = (c: string) => /[A-Za-z0-9_.]/.test(c);

/** Tokenize one source line. Lossless: `tokens.map(t => t.text).join("") === line`. */
export function tokenize(line: string): Token[] {
  const out: Token[] = [];
  const n = line.length;
  let i = 0;
  while (i < n) {
    const c = line[i];

    if (isWs(c)) {
      let j = i;
      while (j < n && isWs(line[j])) j++;
      out.push({ text: line.slice(i, j), kind: "ws" });
      i = j;
    } else if (c === ";") {
      out.push({ text: line.slice(i), kind: "comment" });
      break;
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && line[j] !== '"') j++;
      if (j < n) j++; // include the closing quote
      out.push({ text: line.slice(i, j), kind: "string" });
      i = j;
    } else if (c === "$") {
      let j = i + 1;
      while (j < n && isHex(line[j])) j++;
      out.push({ text: line.slice(i, j), kind: "number" });
      i = j;
    } else if (c === "%") {
      if (i + 1 < n && (line[i + 1] === "0" || line[i + 1] === "1")) {
        let j = i + 1;
        while (j < n && (line[j] === "0" || line[j] === "1")) j++;
        out.push({ text: line.slice(i, j), kind: "number" });
        i = j;
      } else {
        out.push({ text: c, kind: "punct" }); // modulo operator
        i++;
      }
    } else if (c === "@") {
      out.push({ text: c, kind: "number" }); // current-address symbol
      i++;
    } else if (isDigit(c)) {
      let j = i;
      if (line[i] === "0" && (line[i + 1] === "x" || line[i + 1] === "X")) {
        j = i + 2;
        while (j < n && isHex(line[j])) j++;
      } else {
        while (j < n && isDigit(line[j])) j++;
      }
      out.push({ text: line.slice(i, j), kind: "number" });
      i = j;
    } else if (isIdentStart(c)) {
      let j = i;
      while (j < n && isIdent(line[j])) j++;
      const text = line.slice(i, j);
      if (line[j] === ":") {
        out.push({ text, kind: "label" });
      } else {
        const up = text.toUpperCase();
        const kind: TokenKind = DIRECTIVES.has(up)
          ? "directive"
          : MNEMONICS.has(up)
            ? "mnemonic"
            : REGISTERS.has(up)
              ? "register"
              : "ident";
        out.push({ text, kind });
      }
      i = j;
    } else {
      out.push({ text: c, kind: "punct" });
      i++;
    }
  }
  return out;
}

/** Whether a source line assembles to an executable instruction (so it has a
 *  cycle cost): a mnemonic after an optional label — not a directive, a
 *  label-only line, a comment, or blank. */
export function isInstructionLine(line: string): boolean {
  const sig = tokenize(line).filter((t) => t.kind !== "ws");
  let i = 0;
  if (sig[i]?.kind === "label") {
    i++;
    if (sig[i]?.text === ":") i++;
  }
  return sig[i]?.kind === "mnemonic";
}
