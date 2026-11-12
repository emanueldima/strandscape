WASM_BINDGEN ?= wasm-bindgen

.PHONY: web serve check install-tools

web:
	cargo build --release --target wasm32-unknown-unknown
	$(WASM_BINDGEN) --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/strandscape.wasm

serve:
	lsof -tiTCP:8080 -sTCP:LISTEN | xargs kill 2>/dev/null || true
	python3 -m http.server 8080 --directory web

check:
	cargo fmt --check
	cargo check --target wasm32-unknown-unknown
	cargo clippy --target wasm32-unknown-unknown -- -D warnings
	cargo clippy --all-targets -- -D warnings
	cargo test --lib

install-tools:
	rustup target add wasm32-unknown-unknown
	cargo install wasm-bindgen-cli --version 0.2.126 --locked
