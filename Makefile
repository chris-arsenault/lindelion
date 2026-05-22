.PHONY: ci fmt fmt-check clippy test check macos-check bundle-macos

ci: check

check:
	cargo run -p xtask -- check

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --release -- -D warnings -W clippy::cognitive_complexity

test:
	cargo test --workspace

macos-check:
	cargo check -p resonator-synth --target aarch64-apple-darwin

bundle-macos:
	cargo run -p xtask -- bundle resonator-synth --target aarch64-apple-darwin
