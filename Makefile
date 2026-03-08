LSP_BIN_NAME   := update-versions-lsp
LSP_RELEASE    := target/release/$(LSP_BIN_NAME)
EXT_BIN_DIR    := extension/bin

.PHONY: setup build-lsp install-dev lint fmt clean

## Install required toolchain and tools (run once after cloning).
setup:
	rustup target add wasm32-wasip1
	@mkdir -p .git/hooks
	@printf '#!/bin/sh\nexport PATH="$$HOME/.cargo/bin:$$PATH"\ncargo fmt --all -- --check && cargo clippy --manifest-path lsp-server/Cargo.toml -- -D warnings && cargo clippy --manifest-path extension/Cargo.toml --target wasm32-wasip1 -- -D warnings\n' > .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@printf '#!/bin/sh\nexport PATH="$$HOME/.cargo/bin:$$PATH"\ncargo test --manifest-path lsp-server/Cargo.toml && cargo build --manifest-path extension/Cargo.toml --target wasm32-wasip1\n' > .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "Setup complete. Git hooks installed."

## Format and lint both crates.
lint:
	cargo fmt --all -- --check
	cargo clippy --manifest-path lsp-server/Cargo.toml -- -D warnings
	cargo clippy --manifest-path extension/Cargo.toml --target wasm32-wasip1 -- -D warnings

## Auto-format both crates.
fmt:
	cargo fmt --workspace

## Build the native LSP server binary (release mode).
build-lsp:
	cargo build --release -p update-versions-lsp

ZED_WORK_DIR   := $(HOME)/Library/Application Support/Zed/extensions/work/update-versions

## Build the LSP server and copy the binary into extension/bin/ and Zed's work
## directory so that the dev extension can find it without a GitHub download.
install-dev: build-lsp
	mkdir -p $(EXT_BIN_DIR)
	cp $(LSP_RELEASE) $(EXT_BIN_DIR)/$(LSP_BIN_NAME)
	@if [ -d "$(ZED_WORK_DIR)" ]; then \
		mkdir -p "$(ZED_WORK_DIR)/bin"; \
		cp $(LSP_RELEASE) "$(ZED_WORK_DIR)/bin/$(LSP_BIN_NAME)"; \
		echo "Binary also installed to Zed work dir"; \
	fi
	@echo "Binary installed at $(EXT_BIN_DIR)/$(LSP_BIN_NAME)"
	@echo "Now open Zed and run: zed: install dev extension → pick extension/"

## Remove build artefacts.
clean:
	cargo clean
	rm -rf $(EXT_BIN_DIR)
