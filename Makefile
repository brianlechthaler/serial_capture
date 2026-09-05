.PHONY: test lint coverage fmt

test:
	cargo test --all-targets

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

coverage:
	cargo llvm-cov --all-targets --fail-under-functions 100 --fail-under-lines 99
