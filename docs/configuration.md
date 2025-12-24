# pinit configuration guide

This guide documents how `pinit` discovers configuration, resolves templates, and applies them.
It is intentionally exhaustive. :smile:

---

**Contents**

<!-- toc -->

* [1. Config discovery and precedence](#1-config-discovery-and-precedence)
* [2. Supported formats (TOML and YAML)](#2-supported-formats-toml-and-yaml)
* [3. Top-level configuration keys](#3-top-level-configuration-keys)
* [4. Template resolution rules (names, targets, recipes)](#4-template-resolution-rules-names-targets-recipes)
* [5. Sources](#5-sources)
  * [5.1 Local sources](#51-local-sources)
  * [5.2 Git sources](#52-git-sources)
  * [5.3 Cache location for git sources](#53-cache-location-for-git-sources)
* [6. Templates](#6-templates)
  * [6.1 Simple path form](#61-simple-path-form)
  * [6.2 Detailed form (with source)](#62-detailed-form-with-source)
  * [6.3 Path-only templates (no config)](#63-path-only-templates-no-config)
* [7. Targets (template stacks)](#7-targets-template-stacks)
  * [7.1 Override rules](#71-override-rules)
* [8. Recipes (templates + inline file sets)](#8-recipes-templates--inline-file-sets)
* [9. License injection](#9-license-injection)
  * [9.1 Simple form (string)](#91-simple-form-string)
  * [9.2 Detailed form](#92-detailed-form)
* [10. Apply behavior that affects configuration](#10-apply-behavior-that-affects-configuration)
  * [10.1 Apply by name vs path](#101-apply-by-name-vs-path)
  * [10.2 Merge strategy and flags](#102-merge-strategy-and-flags)
  * [10.3 Git ignore behavior](#103-git-ignore-behavior)
* [11. Combinations and real-world setups](#11-combinations-and-real-world-setups)
  * [11.1 Minimal local setup](#111-minimal-local-setup)
  * [11.2 Local templates in multiple directories](#112-local-templates-in-multiple-directories)
  * [11.3 Mixed local + GitHub templates](#113-mixed-local--github-templates)
  * [11.4 Stack templates per language (targets)](#114-stack-templates-per-language-targets)
  * [11.5 Full-stack recipe (composed of templates)](#115-full-stack-recipe-composed-of-templates)
  * [11.6 Config stored in a GitHub repo (team-shared)](#116-config-stored-in-a-github-repo-team-shared)
  * [11.7 Per-project local config (not in ~/.config/pinit)](#117-per-project-local-config-not-in-configpinit)
  * [11.8 Pinning templates to a specific commit](#118-pinning-templates-to-a-specific-commit)
* [12. Troubleshooting and edge cases](#12-troubleshooting-and-edge-cases)
  * ["unknown template" errors](#unknown-template-errors)
  * ["template uses a relative path but has no source"](#template-uses-a-relative-path-but-has-no-source)
  * [License output errors](#license-output-errors)
  * [Merge "skips" when you expected merges](#merge-skips-when-you-expected-merges)
  * [Git ignore behavior surprises](#git-ignore-behavior-surprises)
* [13. YAML equivalents](#13-yaml-equivalents)
* [14. Quick checklist](#14-quick-checklist)

<!-- tocstop -->

---

## 1. Config discovery and precedence

`pinit` loads exactly one config file. It never merges multiple config files.

Search order (highest priority first):

1. `--config <path>` (explicit override)
2. If `XDG_CONFIG_HOME` is set:
   - `XDG_CONFIG_HOME/pinit/pinit.toml`
   - `XDG_CONFIG_HOME/pinit/pinit.yaml`
   - `XDG_CONFIG_HOME/pinit/pinit.yml`
3. Otherwise (HOME fallback):
   - `~/.config/pinit/pinit.toml`
   - `~/.config/pinit/pinit.yaml`
   - `~/.config/pinit/pinit.yml`

Notes:
- If `--config` is used, discovery stops there even if the path does not exist.
- If the config extension is not `toml`/`yaml`/`yml`, `pinit` tries TOML first, then YAML.
- `pinit list` reports "no config found" if nothing is discovered.
- `pinit apply <name>` / `pinit new <name>` require a config (since the name must resolve).
- `pinit apply <path>` bypasses config resolution entirely (see "Apply by name vs path").

---

## 2. Supported formats (TOML and YAML)

`pinit` supports TOML and YAML.

YAML specifics:
- The root must be a mapping (object). Sequences at the root are rejected.
- Scalar values are coerced to strings when strings are expected:
  - `true`, `false`, `42`, `3.14` are accepted for string fields and converted to `"true"`, `"false"`, `"42"`, `"3.14..."`.
- Non-matching shapes are ignored rather than hard-failing (e.g., a bad `targets` entry).

TOML specifics:
- Standard `toml` parsing is used.

---

## 3. Top-level configuration keys

Top-level keys and their meanings:

| Key        | Type                                | Purpose |
|------------|-------------------------------------|---------|
| `base_template` | string                          | Template name automatically prepended when applying a template *by name* |
| `license`  | string or object                     | Optional SPDX-based license injection |
| `sources`  | array of source objects              | Local or git-backed template roots |
| `templates`| map of template definitions          | Named template directories |
| `targets`  | map of template arrays or objects    | Named stacks of templates (optionally with overrides) |
| `recipes`  | map of recipe objects                | Named stacks + (optionally) inline file sets |
| `overrides`| array of override rules              | Default override rules applied to all stacks |

Each section is detailed below.

---

## 4. Template resolution rules (names, targets, recipes)

When you pass a name to `pinit apply` or `pinit new`, `pinit` resolves it in this order:

1. If a recipe exists with that name, it is used (templates + filesets).
2. Else, if a target exists with that name, it is used (template stack).
3. Else, if a template exists with that name, it is used.

```pikchr
down
S: box "Input name"
R: diamond "Recipe exists?"
T: diamond "Target exists?"
M: diamond "Template exists?"
C: diamond "base_template set\nand != name?"
A1: box "Apply template"

Ryes: box "Use recipe" at 3 right of R
Tyes: box "Use target" at 3 right of T
E: box "Error: unknown name" at 3 right of M
P: box "Prepend base_template" at 3 right of C
A2: box "Apply template stack" at 0.9 below P

arrow from S.s to R.n
arrow from R.e to Ryes.w "yes"
arrow from R.s to T.n "no"
arrow from T.e to Tyes.w "yes"
arrow from T.s to M.n "no"
arrow from M.s to C.n "yes"
arrow from M.e to E.w "no"
arrow from C.e to P.w "yes"
arrow from C.s to A1.n "no"
arrow from P.s to A2.n
```

If the name is a template (not a target or recipe):
- If `base_template` is set and not equal to the template name, `base_template` is prepended.
- If `base_template` points to a non-existent template, resolution will fail later when templates are resolved.

If the name is a target or a recipe:
- `base_template` is **not** automatically inserted. You must include it explicitly.

Ordering is preserved. If you define `targets.rust = ["common", "rust"]`, `common` is applied first, then `rust`.

---

## 5. Sources

Sources describe *where* templates are stored. A template can resolve from either:

- a **local path** (`path`)
- a **git repository** (`repo`) with optional `ref` and `subdir`

Source object fields:

| Field   | Type   | Meaning |
|---------|--------|---------|
| `name`  | string | Source identifier used by templates |
| `path`  | string | Local root directory |
| `repo`         | string | Git repo URL or path |
| `git_protocol`| string | `ssh` or `https` for GitHub shorthand repos (default: `ssh`) |
| `ref`          | string | Git ref (`HEAD`, branch, tag, commit) |
| `subdir`       | string | Subdirectory inside the repo |

Resolution rules:
- If `path` is set, `pinit` uses it and ignores `repo`.
- If `path` is not set, `repo` must be set or resolution fails.
- Template paths are resolved relative to the source root.

### 5.1 Local sources

```toml
[[sources]]
name = "local"
path = "/Users/me/templates"
```

Template:

```toml
[templates]
rust = { source = "local", path = "rust" }
```

Resolved directory:

```
/Users/me/templates/rust
```

### 5.2 Git sources

```toml
[[sources]]
name = "remote"
repo = "acme/pinit-templates"
git_protocol = "ssh"    # optional; default is ssh
ref = "v2.3.1"           # branch/tag/commit; defaults to HEAD
subdir = "templates"     # optional
```

Template:

```toml
[templates]
node = { source = "remote", path = "node" }
```

Resolution:
- Repository is cloned into the cache.
- `ref` is checked out in **detached HEAD** mode.
- If `ref` is a branch name and checkout fails, `origin/<ref>` is attempted.

GitHub shorthand:
- If `repo` is in the form `owner/name` (for example `acme/pinit-templates`), `pinit` assumes GitHub.
- By default it expands to `git@github.com:owner/name.git`.
- Set `git_protocol = "https"` to expand to `https://github.com/owner/name.git` instead.

### 5.3 Cache location for git sources

`pinit` uses the platform cache dir (XDG base dirs):
- Linux: `~/.cache/pinit`
- macOS: `~/Library/Caches/pinit`
- Windows: `%LOCALAPPDATA%/pinit`

The path includes a hash of `repo + ref`:

```
<cache>/pinit/repos/<blake3>/repo
```

`pinit` runs `git fetch --tags --prune origin` before checking out the requested ref.

---

## 6. Templates

Templates map a name to a directory. They can be defined in one of two forms.

### 6.1 Simple path form

```toml
[templates]
rust = "/Users/me/templates/rust"
```

Rules:
- If the path is absolute, it is used as-is.
- If the path is relative, you **must** specify a source using the detailed form (see below).

### 6.2 Detailed form (with source)

```toml
[templates]
common = { source = "local", path = "common" }
rust = { source = "local", path = "rust" }
```

Rules:
- `source` selects from `sources`.
- `path` is relative to the source root.

### 6.3 Path-only templates (no config)

You can bypass config entirely by passing a directory path to the CLI:

```
pinit apply /path/to/template
```

In this mode:
- No config is loaded to resolve templates.
- `base_template` does not apply.
- `license` injection does not run.

---

## 7. Targets (template stacks)

Targets are named stacks of template names:

```toml
[targets]
rust = ["common", "rust", "github-actions"]
```

You can also use a detailed form to add overrides:

```toml
[targets.rust]
templates = ["common", "rust"]

[[targets.rust.overrides]]
pattern = ".gitignore"
action = "overwrite"
```

Calling:

```
pinit apply rust
```

applies templates in order: `common`, then `rust`, then `github-actions`.

Notes:
- Targets do not automatically include `base_template`. You must list it explicitly.
- If any template name in a target is missing, the run fails.

### 7.1 Override rules

Override rules let later templates in a stack win for specific paths (without prompting).

Rules:
- Each rule has a `pattern` (or `path`) and an `action` (`overwrite`, `merge`, `skip`).
- `action` defaults to `overwrite` when omitted.
- Rules are checked in order and the **last match wins**.
- `merge` falls back to `skip` when no merge driver is available.
- Matching rules apply without prompting, even in interactive runs.

You can define overrides at three levels:

```toml
[[overrides]]
pattern = ".editorconfig"
action = "skip"

[targets.rust]
templates = ["common", "rust"]

[[targets.rust.overrides]]
pattern = ".gitignore"
action = "overwrite"

[recipes.full]
templates = ["rust"]

[[recipes.full.overrides]]
pattern = "Cargo.toml"
action = "merge"
```

YAML equivalent:

```yaml
overrides:
  - path: ".editorconfig"
    action: skip
targets:
  rust:
    templates: [common, rust]
    overrides:
      - pattern: ".gitignore"
        action: overwrite
recipes:
  full:
    templates: [rust]
    overrides:
      - path: Cargo.toml
        action: merge
```

Pattern notes:
- Patterns are matched against the **relative path** within the template.
- Use `**/` if you want to match nested paths (e.g., `**/.gitignore`).

---

## 8. Recipes (templates + inline file sets)

Recipes can include templates and (optionally) inline file sets.

```toml
[recipes.rust-lite]
templates = ["rust"]

[[recipes.rust-lite.files]]
root = "/Users/me/snippets"
include = ["README.md", ".github/workflows/*.yml"]
dest_prefix = "meta"
```

Recipes also support `overrides` with the same shape as targets and global overrides.

Current behavior (important):
- The config format supports `files`, and they are parsed and resolved.
- The CLI currently **does not apply** file sets. It only applies `templates`.
  This means `files` entries are effectively a no-op in current `pinit` CLI runs.

If you rely on file sets, confirm support in your `pinit` version before using them.

---

## 9. License injection

`pinit` can render SPDX licenses into your project at apply time.

### 9.1 Simple form (string)

```toml
license = "MIT"
```

### 9.2 Detailed form

```toml
[license]
spdx = "Apache-2.0"
output = "LICENSES/Apache-2.0.txt"  # must be relative
year = "2025"
name = "Jane Developer"
args = { "project" = "Acme Tools" }
```

Rules:
- `output` must be a **relative** path; absolute paths cause an error.
- Default output path is `LICENSE`.
- `year` and `name` are convenience fields used to fill SPDX variables:
  - `year` -> `year`
  - `name` -> `fullname` and `copyright holders`
- `args` provides arbitrary SPDX template variables.
- If an SPDX template variable is required but not provided, `pinit` errors.

Where it applies:
- License injection only happens when a template is resolved **by name**.
- If you run `pinit apply /path/to/template`, the license is **not** injected.

---

## 10. Apply behavior that affects configuration

These behaviors are not config keys but explain how configuration is used.

### 10.1 Apply by name vs path

| CLI input | Config loaded? | `base_template` applies? | `license` applies? |
|----------|----------------|-------------------|--------------------|
| `pinit apply rust` | yes | yes (if `base_template` set) | yes |
| `pinit apply /path/to/template` | no | no | no |

### 10.2 Merge strategy and flags

When files already exist in the destination:

- Default action is **merge** if possible.
- If merge is not available for a file, merge falls back to **skip**.
- You can override behavior with flags:
  - `--overwrite`
  - `--merge`
  - `--skip`
  - `--override <glob>` (repeatable) with optional `--override-action <overwrite|merge|skip>`
- `--yes` makes the run non-interactive and applies the selected behavior to all files.

Merge availability:
- Structured merges exist for many file types (TOML, YAML, Rust, JS, TS, PHP, Python, CSS, etc.).
- Unrecognized types are merged line-by-line (additive, de-duplicated).
- Binary or non-UTF-8 files cannot be merged (treated as "merge unavailable").

### 10.3 Git ignore behavior

If the destination is a git worktree:
- `pinit` uses `git check-ignore` to skip ignored files.
- `.git` directories and `.DS_Store` are always ignored.

---

## 11. Combinations and real-world setups

Below are examples tailored for a developer working with Rust, JavaScript, PHP, and Python.

### 11.1 Minimal local setup

Use a single local folder of templates:

```toml
[templates]
rust = "/Users/me/templates/rust"
node = "/Users/me/templates/node"
php = "/Users/me/templates/php"
python = "/Users/me/templates/python"
```

Apply:

```
pinit apply rust
```

### 11.2 Local templates in multiple directories

```toml
[[sources]]
name = "lang"
path = "/Users/me/templates/lang"

[[sources]]
name = "ops"
path = "/Users/me/templates/ops"

[templates]
common = { source = "ops", path = "common" }
rust = { source = "lang", path = "rust" }
node = { source = "lang", path = "node" }
php = { source = "lang", path = "php" }
python = { source = "lang", path = "python" }

base_template = "common"
```

`pinit apply rust` applies `common` + `rust`.

### 11.3 Mixed local + GitHub templates

```toml
base_template = "common"

[[sources]]
name = "local"
path = "/Users/me/templates"

[[sources]]
name = "remote"
repo = "https://github.com/acme/pinit-templates.git"
ref = "v1.8.0"
subdir = "templates"

[templates]
common = { source = "local", path = "common" }
rust = { source = "local", path = "rust" }
node = { source = "remote", path = "node" }
php = { source = "remote", path = "php" }
python = { source = "remote", path = "python" }
```

### 11.4 Stack templates per language (targets)

```toml
[targets]
rust = ["common", "rust", "github-actions", "editorconfig"]
node = ["common", "node", "eslint", "prettier"]
php = ["common", "php", "phpstan"]
python = ["common", "python", "ruff"]
```

Then:

```
pinit apply rust
pinit apply node
```

### 11.5 Full-stack recipe (composed of templates)

```toml
[recipes.fullstack]
templates = ["common", "node", "php", "docker"]
```

Run:

```
pinit apply fullstack
```

### 11.6 Config stored in a GitHub repo (team-shared)

If your team keeps a shared config in a repo:

```
repo/
  pinit.toml
  templates/
    rust/
    node/
```

Use:

```
pinit --config ./pinit.toml apply rust
```

This works even if the config is not in `~/.config/pinit`.

### 11.7 Per-project local config (not in ~/.config/pinit)

You can store a project-specific config next to a project and use it only when needed:

```
~/projects/acme-api/pinit.toml
```

Apply with:

```
pinit --config ~/projects/acme-api/pinit.toml apply rust
```

### 11.8 Pinning templates to a specific commit

```toml
[[sources]]
name = "pinned"
repo = "https://github.com/acme/pinit-templates.git"
ref = "5f2c9bf2b8e4c8b7b2f2f1b5fdd7f7b3f0c9a0d1"

[templates]
rust = { source = "pinned", path = "rust" }
```

---

## 12. Troubleshooting and edge cases

### "unknown template" errors
- The name you passed does not match a recipe, target, or template.
- Check `pinit list` to see what the loaded config provides.

### "template uses a relative path but has no source"
- Use the detailed template form with `source = "..."`
- Or make the template path absolute.

### License output errors
- `license.output` must be a relative path. Absolute paths are rejected.

### Merge "skips" when you expected merges
- Merge is file-type dependent; binary or unsupported files cannot be merged.
- In non-interactive mode, `--merge` falls back to `skip` if merge is not available.

### Git ignore behavior surprises
- If the destination is a git repo and a path is ignored, `pinit` will not copy it.

---

## 13. YAML equivalents

All examples above can be expressed in YAML. Example:

```yaml
base_template: common
sources:
  - name: local
    path: /Users/me/templates
  - name: remote
    repo: https://github.com/acme/pinit-templates.git
    ref: v1.8.0
    subdir: templates
templates:
  common: { source: local, path: common }
  rust: { source: local, path: rust }
  node: { source: remote, path: node }
targets:
  rust: [common, rust]
  node: [common, node]
license:
  spdx: MIT
  year: "2025"
  name: Jane Developer
  output: LICENSE
```

---

## 14. Quick checklist

- Decide where your templates live (local paths, git repos, or both).
- Use `sources` for reusable roots; use `templates` for individual directories.
- Use `base_template` for a baseline, or `targets` for explicit stacks.
- Use `--config` to point at repo-local configs outside `~/.config/pinit`.
- If you need a license file, configure `license` and apply by name.
