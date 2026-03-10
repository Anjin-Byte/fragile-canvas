.PHONY: build run debug test clean

build:
	cargo build --release

run: build
	cargo run --release -p fragile-canvas

debug: build
	cargo run --release -p fragile-canvas -- --debug

test:
	cargo test -p sm83

clean:
	cargo clean
