# pinit
> just-over-engineered-enough **p**roject **init**ialization

`pinit` applies project template baselines to existing (or non-existent) directories. It’s a
copy-and-merge tool with opinions about safety, not a scaffolding wizard with a crystal ball.

---

**Contents**

<!-- toc -->

* [Features](#features)
* [Installation](#installation)
* [Quick start](#quick-start)
* [Usage](#usage)
* [Template model (sources → templates → targets → recipes)](#template-model-sources-%E2%86%92-templates-%E2%86%92-targets-%E2%86%92-recipes)
* [Configuration](#configuration)
* [Common workflows](#common-workflows)
* [Documentation](#documentation)
* [Project status](#project-status)
* [Development](#development)
* [Contributing](#contributing)
* [License](#license)

<!-- tocstop -->

---

## Features

- Stack multiple templates in order (targets/recipes).
- Merge conservatively for common file types; fall back to skip when unsafe.
- Override per-path precedence for “this file always wins.”
- Dry-run and non-interactive modes for automation.
- Respects destination `.gitignore` rules.
- Optional SPDX license injection.

## Installation

Prebuilt binaries:

```sh
brew install claylo/brew/pinit
```

```sh
cargo binstall pinit
```

From a local checkout:

```sh
cargo install --path crates/pinit
```

Or run without installing:

```sh
cargo run -p pinit -- --help
```

## Quick start

```sh
# list configured templates/recipes
pinit list

# apply a template or recipe into the current directory
pinit apply rust

# create a new directory, init git, and apply a template
pinit new rust myproj
```

## Usage

```text
pinit apply <template|path> [dest] [--dry-run] [--yes] [--overwrite|--merge|--skip] [--override <glob>...] [--override-action <overwrite|merge|skip>]
pinit new <template|path> <dir> [--dry-run] [--yes] [--no-git] [--branch main] [--override <glob>...] [--override-action <overwrite|merge|skip>]
pinit list
```

Notes:
- `--dry-run` computes changes without writing.
- `--yes` makes the run non-interactive (default action is merge when available).
- The selected action handles existing files: overwrite, additive merge, or skip.
- `--override` forces precedence for matching paths (last-wins).
- Destination gitignore rules are honored to avoid copying ignored files.

## Template model (sources → templates → targets → recipes)

- **Sources** point at local directories or git repos.
- **Templates** are named directories inside sources.
- **Targets** are ordered stacks of templates.
- **Recipes** are like targets, plus optional inline file sets.

If you want the long version, see `docs/CONFIG.md`.

## Configuration

`pinit` loads config from `~/.config/pinit/pinit.toml` or `~/.config/pinit/pinit.yaml` (or `--config <path>`).

TOML example:
```toml
base_template = "common"

[[sources]]
name = "local"
path = "/Users/me/src/pinit/templates"

[templates]
common = { source = "local", path = "common" }
rust = { source = "local", path = "rust" }

[targets]
rust = ["common", "rust"]

[recipes.rust-lite]
templates = ["rust"]
```

YAML example:
```yaml
base_template: common
sources:
  - name: local
    path: /Users/me/src/pinit/templates
templates:
  common: { source: local, path: common }
  rust: { source: local, path: rust }
targets:
  rust: [common, rust]
recipes:
  rust-lite:
    templates: [rust]
```

## Common workflows

```sh
# dry-run a stack into the current directory
pinit apply rust --dry-run

# apply a template directory directly (bypasses config)
pinit apply /path/to/template

# force a later template to overwrite a specific file
pinit apply rust --override .gitignore --override-action overwrite
```

## Documentation

- Configuration guide: `docs/CONFIG.md`
- Comparisons: `docs/COMPARISON.md`
- Changelog: `CHANGELOG.md`
- Plan/Roadmap: `PLAN.md`

## Project status

`pinit` is pre-1.0. Expect a fast-moving feature set and a few sharp edges that are being sanded
in real time.

## Development

Validation:

```sh
just check
just test
just cov
```

Xtask:

```sh
# build a manpage into target/man
cargo xtask man

# build and install the CLI into ~/.bin
cargo xtask install
```

## Contributing

Open a PR with a clear intent and keep changes focused. If you touch behavior, add tests in the
same change. (Future you will say thanks.)

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.
