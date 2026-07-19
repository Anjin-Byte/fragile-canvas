//! Linear disassembler CLI.
//!
//! Usage: sm83-disasm <rom> [--start HEX] [--len HEX] [--org HEX]
//!
//! Prints an address/bytes/mnemonic listing. `--start`/`--len` select a
//! byte range of the file; `--org` sets the address of the first byte
//! (defaults to the file offset of `--start`).

use sm83_isa::{decode, format_instruction, FormatOptions};

fn parse_hex(s: &str) -> Result<u32, String> {
    let t = s.trim_start_matches("0x").trim_start_matches('$');
    u32::from_str_radix(t, 16).map_err(|e| format!("bad hex '{s}': {e}"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path: Option<String> = None;
    let mut start: u32 = 0;
    let mut len: Option<u32> = None;
    let mut org: Option<u32> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--start" => start = parse_hex(it.next().expect("--start needs a value")).unwrap(),
            "--len" => len = Some(parse_hex(it.next().expect("--len needs a value")).unwrap()),
            "--org" => org = Some(parse_hex(it.next().expect("--org needs a value")).unwrap()),
            "-h" | "--help" => {
                eprintln!("usage: sm83-disasm <rom> [--start HEX] [--len HEX] [--org HEX]");
                return;
            }
            other => path = Some(other.to_string()),
        }
    }

    let path = path.unwrap_or_else(|| {
        eprintln!("usage: sm83-disasm <rom> [--start HEX] [--len HEX] [--org HEX]");
        std::process::exit(2);
    });
    let data = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1);
    });

    let start = start as usize;
    let end = len
        .map(|l| (start + l as usize).min(data.len()))
        .unwrap_or(data.len());
    let base = org.unwrap_or(start as u32) as u16;

    let slice = &data[start.min(data.len())..end];
    let mut offset = 0usize;
    while offset < slice.len() {
        let addr = base.wrapping_add(offset as u16);
        let d = decode(&slice[offset..]).expect("non-empty");
        let raw: Vec<String> = slice[offset..offset + d.len as usize]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        let text = format_instruction(&d.instr, &FormatOptions { addr: Some(addr), symbols: None });
        println!("{addr:04X}  {:<8}  {text}", raw.join(" "));
        offset += d.len as usize;
    }
}
