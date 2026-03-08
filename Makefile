LSP_BIN_NAME   := update-versions-lsp
LSP_RELEASE    := target/release/$(LSP_BIN_NAME)
EXT_BIN_DIR    := extension/bin

.PHONY: setup build-lsp install-dev lint fmt clean release release-patch release-minor release-major _do-release

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

## ── Release ──────────────────────────────────────────────────────────────────
# Current workspace version read from Cargo.toml.
CURRENT_VERSION := $(shell grep '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

## Finalize a release for the version already set in Cargo.toml (no bump).
release:
	@$(MAKE) --no-print-directory _do-release

## Bump patch (x.y.Z+1), commit, tag, and push.
release-patch:
	$(eval _NEXT := $(shell echo "$(CURRENT_VERSION)" | awk -F. '{print $$1"."$$2"."$$3+1}'))
	@sed -i.bak 's/^version = "$(CURRENT_VERSION)"/version = "$(_NEXT)"/' Cargo.toml && rm -f Cargo.toml.bak
	@$(MAKE) --no-print-directory _do-release

## Bump minor (x.Y+1.0), commit, tag, and push.
release-minor:
	$(eval _NEXT := $(shell echo "$(CURRENT_VERSION)" | awk -F. '{print $$1"."$$2+1".0"}'))
	@sed -i.bak 's/^version = "$(CURRENT_VERSION)"/version = "$(_NEXT)"/' Cargo.toml && rm -f Cargo.toml.bak
	@$(MAKE) --no-print-directory _do-release

## Bump major (X+1.0.0), commit, tag, and push.
release-major:
	$(eval _NEXT := $(shell echo "$(CURRENT_VERSION)" | awk -F. '{print $$1+1".0.0"}'))
	@sed -i.bak 's/^version = "$(CURRENT_VERSION)"/version = "$(_NEXT)"/' Cargo.toml && rm -f Cargo.toml.bak
	@$(MAKE) --no-print-directory _do-release

# Internal: generate changelog, regenerate lockfile, commit, tag, push.
# Re-reads version from Cargo.toml so it always sees the bumped value.
_do-release:
	$(eval _VER := $(shell grep '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/'))
	@echo "Releasing v$(_VER) ..."
	@DIRTY=$$(git status --porcelain | grep -v '^??' | grep -Ev ' (Cargo\.toml|Cargo\.lock)$$'); \
		[ -z "$$DIRTY" ] || { printf "ERROR: uncommitted changes — stash or commit first:\n%s\n" "$$DIRTY"; exit 1; }
	@cargo generate-lockfile
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore(release): v$(_VER)"
	@git tag "update-versions-lsp-v$(_VER)"
	@git push origin main
	@git push origin "update-versions-lsp-v$(_VER)"
	@echo "Released v$(_VER). GitHub Actions will build the binaries."
