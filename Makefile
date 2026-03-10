.PHONY: build run debug test clean desktop wasm web

build:
	cargo build --release

run: build
	cargo run --release -p fragile-canvas

debug: build
	cargo run --release -p fragile-canvas -- --debug

test:
	cargo test -p sm83

desktop:
	cd desktop && npm run tauri dev

wasm:
	cd wasm && wasm-pack build --target web

web: wasm
	cd web && npm run dev

clean:
	cargo clean
	rm -rf wasm/pkg
