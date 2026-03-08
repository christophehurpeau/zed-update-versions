//! update-versions — WASM extension
//!
//! Thin wrapper whose only job is to locate the native LSP server binary and
//! hand its path to Zed.  All intelligence lives in `update-versions-lsp`.
//!
//! # Development workflow
//!
//! 1. Build the native binary:   `make build-lsp`
//! 2. Copy it into the bin dir:  `make install-dev`
//! 3. In Zed: "zed: install dev extension" → pick the `extension/` folder.
//!
//! The binary is expected at `extension/bin/update-versions-lsp`
//! (or `update-versions-lsp.exe` on Windows).

use zed_extension_api::{self as zed, LanguageServerId, Os, Result, Worktree};

struct UpdateVersionsExtension {
    /// Cache the resolved path so we don't hit the filesystem on every call.
    cached_binary_path: Option<String>,
}

impl zed::Extension for UpdateVersionsExtension {
    fn new() -> Self {
        UpdateVersionsExtension {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.find_binary()?,
            args: vec![],
            env: vec![],
        })
    }
}

impl UpdateVersionsExtension {
    /// Return the path to the LSP binary, using a simple cache.
    ///
    /// For the POC the binary is located in the `bin/` subdirectory of the
    /// extension *source* folder (populated by `make install-dev`).  The
    /// absolute path is baked in at compile time via `CARGO_MANIFEST_DIR` so
    /// it works regardless of what working directory Zed uses when running the
    /// extension host.
    ///
    /// A future production version will download the correct pre-built binary
    /// from GitHub Releases on first use.
    fn find_binary(&mut self) -> Result<String> {
        if let Some(ref path) = self.cached_binary_path {
            return Ok(path.clone());
        }

        let (os, _arch) = zed::current_platform();
        let binary_name = match os {
            Os::Windows => "update-versions-lsp.exe",
            _ => "update-versions-lsp",
        };

        // Absolute path baked in at compile time so it resolves correctly
        // regardless of Zed's working directory for the extension host.
        // The WASM sandbox blocks std::fs checks, so we return the path
        // directly — Zed will report a clear error if the binary is absent.
        let binary_path = format!("{}/bin/{binary_name}", env!("CARGO_MANIFEST_DIR"));
        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

zed::register_extension!(UpdateVersionsExtension);
