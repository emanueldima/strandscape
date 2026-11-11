WASM_BINDGEN ?= wasm-bindgen

.PHONY: web serve check

web:
	cargo build --release --target wasm32-unknown-unknown
	$(WASM_BINDGEN) --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/strandscape.wasm

serve:
	python3 -m http.server 8080 --directory web

check:
	cargo fmt --check
	cargo check --target wasm32-unknown-unknown
	cargo clippy --target wasm32-unknown-unknown -- -D warnings
	cargo clippy --all-targets -- -D warnings
	cargo test --lib
