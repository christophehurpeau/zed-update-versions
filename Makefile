LSP_BIN_NAME   := update-versions-lsp
LSP_RELEASE    := lsp-server/target/release/$(LSP_BIN_NAME)
EXT_BIN_DIR    := extension/bin

.PHONY: build-lsp install-dev clean

## Build the native LSP server binary (release mode).
build-lsp:
	cargo build --release --manifest-path lsp-server/Cargo.toml

## Build the LSP server and copy the binary into extension/bin/
## so that the dev extension can find it.
install-dev: build-lsp
	mkdir -p $(EXT_BIN_DIR)
	cp $(LSP_RELEASE) $(EXT_BIN_DIR)/$(LSP_BIN_NAME)
	@echo "Binary installed at $(EXT_BIN_DIR)/$(LSP_BIN_NAME)"
	@echo "Now open Zed and run: zed: install dev extension → pick extension/"

## Remove build artefacts.
clean:
	cargo clean --manifest-path lsp-server/Cargo.toml
	rm -rf $(EXT_BIN_DIR)
