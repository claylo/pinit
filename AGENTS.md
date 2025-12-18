# Repository Guidelines

## Project Structure
- Consider the project a monorepo containing multiple crates unless explicitly told otherwise.
- Follow the existing crate/workspace layout. If creating a new workspace, prefer a simple, discoverable structure (e.g. crates under `crates/`).
- Treat `target/` as build output only; do not commit artifacts.
- Capture architectural decisions in Markdown ADRs under `docs/decisions/`.

## Build, Test, and Development Commands
- `just --list` — show available recipes (preferred entrypoint if `just` is installed).
- `just check` — format, lint, tests (nextest), and doctests.
- `just fmt` — format.
- `just clippy` — lint (CI-level strict).
- `just test` (or `cargo nt`) — tests (requires `cargo-nextest`).
- `just doc-test` (or `cargo doc-test`) — doctests.
- `cargo check --all-targets --all-features` — fast compile sanity check.
- Optional: `just cov` (or `cargo cov`) — coverage (requires `cargo-llvm-cov`).
- Optional: `markdownlint` — lint Markdown files.

## Coding Style & Naming Conventions
- Rust 2024 edition; `#![forbid(unsafe_code)]` in each crate — keep contributions safe and pure Rust.
- Prefer idiomatic, readable Rust; rely on `rustfmt` defaults.
- Keep clippy clean; avoid `unsafe` unless there’s a clear, documented need.
- Use standard Rust naming: `snake_case` (items), `UpperCamelCase` (types/traits).
- For public APIs: add rustdoc, examples when helpful, and keep semver impact in mind.

## Testing Guidelines
- If you change behavior, add/adjust tests in the same change.
- Prefer fast, deterministic tests; use integration tests for end-to-end behavior.
- `just cov` output should exceed 95% prior to any commit.

## Commit & Pull Request Guidelines
- Follow Conventional Commits (`feat:`, `fix:`, `docs:`, `perf:`, `chore:`) for clarity.
- PRs should include: purpose, scope of files touched, manual checks performed, and open questions.
- Link related issues if available.
- Never make commits directly. Write commit message to `commit.txt`. If branch is clean, make sure `commit.txt` is empty before writing to it. Otherwise, append since changes are accumulating prior to commit.

## MSRV
- If you support an MSRV, set `package.rust-version` in `Cargo.toml` and keep it intentional (see `MSRV.md`).
- CI runs an MSRV compile check when `rust-version` is present.

## Docs
- If you capture architectural decisions, prefer lightweight ADRs under `docs/decisions/`.

## Releases
- Releases are automated via `release-plz` on pushes to `main`.
- Publishing to crates.io requires the `CARGO_REGISTRY_TOKEN` GitHub Actions secret.
- Keep `CHANGELOG.md` up to date for user-visible changes.

## Security & Configuration
- Do not commit secrets. Prefer environment variables for credentials and document configuration defaults.
