# MSRV policy

This repo follows Rust’s **MSRV** (minimum supported Rust version) via the `rust-version` field in `Cargo.toml`.

- Set `package.rust-version` (single-crate) or ensure each published crate has it (workspace).
- CI runs a compile check on that toolchain when `rust-version` is present.
- Bump MSRV intentionally (and document it in `CHANGELOG.md`) when required.

