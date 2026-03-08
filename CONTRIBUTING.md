# Contributing

Thank you for your interest in contributing to zed-update-versions!

---

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain + `wasm32-wasip1` target)
- [Zed](https://zed.dev/) (latest stable or preview)

```sh
rustup target add wasm32-wasip1
```

---

## Repository layout

```
extension/        WASM extension (Rust → .wasm) — thin shim that locates the LSP binary
lsp-server/       Native LSP server binary (Rust)
```

---

## Development workflow

### 1. Build the LSP server

```sh
make build-lsp
```

This compiles `lsp-server/` in release mode and places the binary at `lsp-server/target/release/update-versions-lsp`.

### 2. Install the binary for the dev extension

```sh
make install-dev
```

Copies the binary to `extension/bin/update-versions-lsp` where the WASM extension looks for it at dev time.

### 3. Install the dev extension in Zed

Open Zed, run the command `zed: install dev extension`, and pick the `extension/` directory.

### 4. Full clean rebuild

```sh
make clean && make install-dev
```

---

## Running tests

```sh
# LSP server unit tests
cd lsp-server && cargo test

# Extension (WASM) — build check only (no testable logic)
cd extension && cargo build --target wasm32-wasip1
```

---

## Linting & formatting

The project uses standard Rust tooling. Before opening a PR, please run:

```sh
cd lsp-server
cargo fmt --check
cargo clippy -- -D warnings
```

---

## Submitting changes

1. Fork the repository and create a feature branch.
2. Make sure `cargo test`, `cargo fmt --check`, and `cargo clippy` all pass.
3. Open a pull request against `main` with a clear description of the change.

---

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
