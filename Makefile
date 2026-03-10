BOOT_ROM ?= ROMs/DMG_ROM.bin
CART     ?= ROMs/Tetris.gb

.PHONY: build run debug test clean

build:
	cargo build --release --manifest-path sm83/Cargo.toml

run: build
	cd sm83 && cargo run --release -- "../$(CART)"

debug: build
	cd sm83 && cargo run --release -- --debug "../$(CART)"

test:
	cargo test --manifest-path sm83/Cargo.toml

clean:
	cargo clean --manifest-path sm83/Cargo.toml
