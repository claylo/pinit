# pinit Rust CLI plan

## Goals
- Replace the repo-level `Justfile`/`bin/install-template` flow with a Rust CLI (`pinit`) that installs templates into an existing directory.
- Support template sources from:
  - A local templates directory (e.g. `./templates/` or any user-provided path).
  - A git repository (cloned to a local cache) and an optional subdirectory within it.
- Support user configuration in **either** `~/.config/pinit.toml` **or** `~/.config/pinit.yaml` for:
  - Template source(s)
  - Template name → path mapping
  - A `base_template` value naming the base layer template
  - Target aliases (e.g. `rust` → stack `common + rust`)
- Provide two crates:
  - `pinit` (the `pinit` command)
  - `pinit-core` (library for reuse / alternative CLIs)
- Use license-clean (MIT/Apache/BSD) dependencies. No GPL code or libraries.

## Non-goals (for v1)
- Full 3-way merge / conflict resolution across divergent histories.
- Semantic merges for every file type under the sun.
- Template parameterization / scaffolding (vars, prompts, rendering).
- Remote module registries.

## UX / Commands (target)
- `pinit apply <template> [dest]`
  - Applies `<template>` into `dest` (default `.`).
  - For existing files, default to prompting with choices: overwrite / additive merge / skip.
- `pinit new <template> <dir>`
  - Creates `<dir>`, `git init` (optional flag), then runs `apply`.
- `pinit list`
  - Shows configured template names and where they resolve from.
- Common flags:
  - `-n, --dry-run` (show proposed diffs; write nothing)
  - `-y, --yes` (non-interactive; apply all additive changes)
  - `--config <path>` (override config discovery)
  - `--templates <path|repo>` (override template source for this run)
  - `-v/--verbose`, `--quiet`

## Template model
- A “template” is a directory tree of files.
- A “stack” is an ordered list of templates applied in sequence (e.g. `common` then `rust`).
- Source resolution order:
  1. CLI overrides (`--config`, `--templates`, explicit `--base-template`)
  2. `~/.config/pinit.{toml,yaml}`
  3. Local fallback: `./templates` next to the running binary (or current repo in dev mode).

## Recipe model (config-defined)
- A “recipe” is a configured stack or a pure config-defined file set (for “flavors” within a language).
- Recipes may be composed entirely of file listings drawn from one or more roots:
  - Explicit file list entries
  - Glob patterns (e.g. `**/*.md`, `.github/workflows/*.yml`)
  - Mapped to a destination prefix (optional)
- Recipe composition can combine:
  - Named templates from sources (directory trees)
  - “Inline file sets” from arbitrary paths

## Config shape (sketch)
TOML:
```toml
base_template = "common"

[[sources]]
name = "local"
path = "/Users/me/src/pinit/templates"

[[sources]]
name = "work"
repo = "git@github.com:me/pinit-templates.git"
ref = "main"
subdir = "templates"

[templates]
common = { source = "local", path = "common" }
rust   = { source = "local", path = "rust" }

[targets]
rust = ["common", "rust"]
```

YAML equivalent:
```yaml
base_template: common
sources:
  - name: local
    path: /Users/me/src/pinit/templates
templates:
  common: { source: local, path: common }
  rust:   { source: local, path: rust }
targets:
  rust: [common, rust]
```

## Additive merge policy
Default intent is safe-by-default, with explicit user control per file when there’s an existing destination.

File handling rules (v1):
- If dest path does not exist: copy file (preserve permissions where possible).
- If dest exists and is identical: no-op.
- If dest exists and differs:
  - User choice (or flag-driven default): overwrite / additive merge / skip.
  - For supported formats: additive “insert missing” merge (format-specific; never delete; never overwrite existing values/definitions).
  - For `.env`: add missing keys (do not duplicate existing keys).
  - For `.envrc`: add missing lines without duplicating exports/vars already set.
  - Otherwise: line-based append of missing exact lines.

## “AST merge tool” approach (license-clean)
Implement merges ourselves on top of permissive crates:
- TOML: `toml_edit` (`DocumentMut`) to preserve formatting as much as feasible while inserting missing keys/tables.
- YAML: `yaml-rust2` merge for mappings (insert missing keys; recursively merge maps; do not overwrite scalars).
  - Rationale: `serde_yaml` is deprecated.
  - Note: `yaml-rust2` is young; expect some churn. Before adopting, verify its crate license is permissive (MIT/Apache/BSD) and pin a version.
- Optional later: JSON via `serde_json::Value`.

## Language-aware merging (source code)
Merging needs to extend beyond TOML/YAML because templates may include source code.

Constraints:
- This tool is primarily a “bring me up to baseline” installer (template → existing project), not a full 3‑way merge engine.
- Without a base template, truly correct merges of edited source code are hard; the safe default is to ask the user (overwrite/merge/skip) and show a good diff.

Plan:
- Define a merge-driver registry in `pinit-core` keyed by filename/extension.
- Start with conservative, deterministic drivers (TOML/YAML/env/envrc/line-append).
- Start adding language-aware drivers early (Phase 5) using permissive AST tooling (Tree-sitter) to do “insert missing top-level declarations” merges where feasible, and otherwise fall back to diff + user choice.

Diff output:
- Use a permissive diff crate (e.g. `similar` or `diffy`) to print unified diffs for `--dry-run` and interactive prompts.

## Git template sources
- Clone/fetch via invoking the system `git` (avoid libgit2 linkage/packaging surprises).
- Cache location: `~/.cache/pinit/` (or platform-appropriate via `directories` crate).
- Resolve by `(repo URL, ref)` → checkout directory, then template root is `${checkout}/${subdir}`.

## Security constraints
- Refuse to follow symlinks that escape the template root (and optionally refuse writing through symlinks in dest).
- Normalize and validate paths to prevent `../` traversal from template files.
- Honor ignore rules in the target directory (e.g. `.gitignore`, global git excludes) so we don’t copy/diff ignored files like `.DS_Store`.
- Never print secret values (only filenames/paths); keep logging conservative.

## OSS license injection
- Support a config stanza that selects an SPDX license ID and renders a license file into the destination.
- Use the permissively-licensed `license` crate (repo `https://github.com/evenorog/license`) to render license text and pass template args supported by SPDX data.
  - Examples:
    - MIT: inject year + name
    - Apache-2.0 / GPL: render text as-is (or with supported fields if available)
- Inputs for template args come from config and/or CLI flags; never guess user identity.

---

## Phases of work

### Phase 1: Workspace + crate split
- Create a Cargo workspace with:
  - `crates/pinit-core` (library)
  - `crates/pinit` (binary crate, published as `pinit`)
- `pinit` depends on `pinit-core`; CLI stays thin and delegates to core.
- Add `clap` CLI skeleton in `pinit`.
- Add a minimal end-to-end smoke test in `pinit` (invoke core against temp dirs).

### Phase 2: Config + recipe resolution
- Implement config discovery/parse (`~/.config/pinit.toml|yaml`, `--config`) in `pinit-core`.
  - TOML parsing via `toml`.
- YAML parsing via `yaml-rust2` (convert YAML AST → internal config).
- Implement “recipe” resolution:
  - Targets that expand to stacks (`common + rust`)
  - Inline file-set recipes (explicit + globbed entries from various roots)
- Tests: resolve recipes from config; good errors for missing names/paths.

### Phase 3: Template source resolution
- Implement local directory source (`--templates <path>` and config source with `path`).
- Implement git repo source with cache (`repo`, `ref`, `subdir`).
- Implement template stack resolution for recipes that reference templates.
- Tests: git cache path handling, missing refs, missing template directories.

### Phase 4: File walker + ignore-aware copy engine
- Walk template tree deterministically (stable ordering).
- Apply ignore rules for the *destination* (gitignore + global excludes) to avoid copying/diffing ignored paths.
- Copy missing files (preserve permissions where possible) unless ignored.
- Detect identical files (fast path) unless ignored.
- Tests: install into temp dir; verify files and modes.

### Phase 5: Existing-file behaviors + additive merge strategies (v1)
- For existing differing files, implement behaviors:
  - `overwrite`
  - `merge` (additive)
  - `skip`
  - Behavior can be set via flags and overridden per-file interactively unless `--yes`.
- Implement merge dispatcher by path/extension (merge-driver registry in `pinit-core`):
  - `.toml` via `toml_edit`
- `.yml`/`.yaml` via `yaml-rust2`
  - `.env` key merge
  - `.envrc` safe line merge
  - fallback line-append merge
- Start language-aware merging here using the Tree-sitter ecosystem (permissive licenses) where it can safely do “insert missing” behavior:
  - Parse source with Tree-sitter and attempt to insert missing top-level nodes (best-effort, deterministic).
  - Keep it conservative: never rewrite existing nodes; never attempt clever conflict resolution; always fall back to diff + user choice.
  - Initially prioritize languages you actually template (likely Rust, plus common config-ish formats).
- Implement unified diff preview + interactive prompt per changed file.
- Flags: `--dry-run`, `--yes`.
- Tests: golden tests for each merge strategy (source/dest → merged).

### Phase 5.5: Template precedence overrides (per-file, last-wins)
- Goal: allow later templates in a stack to explicitly override earlier templates for specific paths
  (e.g. keep all of `common`, but force `rust/.gitignore` to replace `common/.gitignore`).
- Config shape:
  - Add an optional override rule list that can live on targets and recipes without breaking existing config:
    - Preserve the current `targets.<name> = [..]` shape.
    - Add a detailed target/recipe form, e.g. `targets.<name> = { templates = [...], overrides = [...] }`.
    - Each override entry includes a path or glob and an action:
      - `path` / `pattern` (string, glob-style)
      - `action` = `overwrite` | `merge` | `skip` (default: `overwrite` for "last-wins")
  - Keep a global override list optional for users who want defaults across all stacks.
- CLI shape:
  - Add a flag like `--override <glob>` (repeatable) and `--override-action <overwrite|merge|skip>`
    to force precedence without editing config.
  - Defaults: `--override` implies `overwrite` unless `--override-action` is provided.
- Core behavior:
  - Extend template-application context to know "current template name" and its stack index.
  - Before prompting/merging, check override rules for the current template and relative path.
  - If a rule matches, choose the requested action and **do not prompt** (even in interactive mode).

### Phase 6: `new` command + git init
- Implement `pinit new <template> <dir>` using the same engine.
- Optional flags: `--no-git`, `--git` (default on), `--branch main`.
- Tests: creates directory; respects `--dry-run` (no mkdir/no git).

### Phase 7: License injection
- Implement license rendering in `pinit-core`:
  - Config selects SPDX ID + template args (year, name, etc.)
  - Output to `LICENSE` (and/or additional configured paths)
- Tests: ensure rendered text includes injected fields for licenses that support them.

### Phase 8: Documentation
- Update `README.md` to document CLI + config examples.
- Add a “how to validate” checklist (or CI for the CLI itself).
- Add comprehensive inline documentation and examples in the source code.
- Use `cargo xtask` with the `clap_mangen` crate to add a workflow for building a manpage
- Add a `cargo xtask` that installs the build target cli in `~/.bin` for testing outside of the development environment.

### Phase 9: Release automation (release-plz)
- Set up release automation for *this repo* (not just the generated templates):
  - Add `release-plz.toml` configured for a workspace with `pinit` + `pinit-core`.
  - Add `.github/workflows/ci.yml` and `.github/workflows/release-plz.yml` modeled after `templates/rust`:
    - CI: fmt, clippy, nextest, doctests, MSRV (when `rust-version` is set), semver checks (if desired).
    - Release: `release-plz` release-pr + release jobs; publish crates using `CARGO_REGISTRY_TOKEN`.
    - Pin GitHub Actions by SHA and add `.github/dependabot.yml` for weekly updates.
  - Ensure `Cargo.toml` metadata is publish-ready (license, repository, readme, categories, keywords) and that `CHANGELOG.md` exists at the workspace root.

### Phase 10: Configurable hook commands (global + recipe)
- Goal: allow users to run commands at specific lifecycle phases, with explicit opt-in for
  init-only vs update runs.
- Supported phases:
  - `after_dir_create`: after `pinit new` creates the directory, before copying.
  - `after_recipe`: after a recipe finishes (recipe-scoped hooks).
  - `after_all`: after all templates/licenses are applied.
- Config shape (TOML):
  - Global hooks under `hooks.<phase>`.
  - Recipe hooks under `recipes.<name>.hooks.<phase>`.
  - Each hook entry:
    - `command` (array of strings, required; no shell mode).
    - `run_on` = `["init"]` or `["update"]` or both (required).
    - `cwd` (optional, default: destination root).
    - `env` (optional map, merged into process env).
    - `allow_failure` (optional, default false).
- YAML mirrors TOML shape.
- Semantics:
  - `run_on` gates execution: init = `pinit new`, update = `pinit apply` to existing dir.
  - `after_dir_create` runs only on init.
  - `after_recipe` runs only for recipe-resolved applies.
  - In `--dry-run`, hooks are not executed; report them in the summary.
- Errors:
  - Hook failures abort unless `allow_failure = true`.
  - Invalid `run_on` values or empty commands are config errors.
- Tests:
  - Parsing for global + recipe hooks in TOML and YAML.
  - Init/update gating.
  - Dry-run skips hooks.
  - Failure handling with/without `allow_failure`.
  - If action is `merge` but merge is unavailable, fall back to `skip` (same as today).
  - Ensure rules only apply to files that are part of the incoming template, not unrelated dest files.
- Tests:
  - Config parsing for new target/recipe shapes in TOML and YAML.
  - Stack apply: later template with override replaces earlier file; non-matching files retain default behavior.
  - CLI override flag works without config changes.
  - Interactive prompt is bypassed when an override rule applies.
- Docs:
  - Update configuration guide with override rules and CLI examples.
  - Document hook phases, configuration options, and examples for both global and recipe hooks.

### Phase 20: Hardening + nice-to-haves
- Symlink policy toggles (`--follow-symlinks` default off).
- Better TOML table ordering / comment preservation where feasible.
- Optional format support (JSON), ignore globs, and `--include/--exclude`.
- Expand language-aware merge drivers for common source files (opt-in or best-effort), with strict fallbacks to diff + user choice.
- Add `--print-config` and `--explain <file>` debug tooling.
