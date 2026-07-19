//! Two-pass assembler CLI.
//!
//! Usage: sm83-asm <source.asm> [-o out.bin] [--fill HEX] [--listing]
//!
//! Assembles to a flat binary starting at the lowest emitted address,
//! padding gaps between ORG segments with the fill byte (default $00).
//! Diagnostics go to stderr; a non-empty diagnostic list is a failure.

use sm83_isa::asm::assemble;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut src_path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut fill: u8 = 0x00;
    let mut listing = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => out_path = Some(it.next().expect("-o needs a path").clone()),
            "--fill" => {
                let v = it.next().expect("--fill needs a hex byte");
                let v = v.trim_start_matches("0x").trim_start_matches('$');
                fill = u8::from_str_radix(v, 16).expect("bad fill byte");
            }
            "--listing" => listing = true,
            "-h" | "--help" => {
                eprintln!("usage: sm83-asm <source.asm> [-o out.bin] [--fill HEX] [--listing]");
                return;
            }
            other => src_path = Some(other.to_string()),
        }
    }

    let src_path = src_path.unwrap_or_else(|| {
        eprintln!("usage: sm83-asm <source.asm> [-o out.bin] [--fill HEX] [--listing]");
        std::process::exit(2);
    });
    let source = std::fs::read_to_string(&src_path).unwrap_or_else(|e| {
        eprintln!("cannot read {src_path}: {e}");
        std::process::exit(1);
    });

    let result = assemble(&source);

    for d in &result.diagnostics {
        eprintln!("{src_path}:{d}");
    }
    if listing {
        print!("{}", result.listing);
    }
    if !result.ok() {
        std::process::exit(1);
    }

    match result.flatten(fill) {
        Some((origin, bytes)) => {
            let out = out_path.unwrap_or_else(|| format!("{src_path}.bin"));
            std::fs::write(&out, &bytes).unwrap_or_else(|e| {
                eprintln!("cannot write {out}: {e}");
                std::process::exit(1);
            });
            eprintln!("wrote {out}: {} bytes at origin ${origin:04X}", bytes.len());
        }
        None => eprintln!("nothing emitted"),
    }
}
