# pinit
> just-over-engineered-enough **p**roject **init**ialization

`pinit` applies project template baselines to existing (or non-existent) directories.

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
pinit apply <template|path> [dest] [--dry-run] [--yes] [--overwrite|--merge|--skip]
pinit new <template|path> <dir> [--dry-run] [--yes] [--no-git] [--branch main]
pinit list
```

Notes:
- `--dry-run` computes changes without writing.
- `--yes` makes the run non-interactive (default action is merge when available).
- The selected action handles existing files: overwrite, additive merge, or skip.
- Destination gitignore rules are honored to avoid copying ignored files.

## Configuration

`pinit` loads config from `~/.config/pinit.toml` or `~/.config/pinit.yaml` (or `--config <path>`).

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

## Validation

```sh
just check
just test
just cov
```

## Xtask

```sh
# build a manpage into target/man
cargo xtask man

# build and install the CLI into ~/.bin
cargo xtask install
```

## Prior Art

It's not like I'm the first to say, "hey, starting a new project should suck less!" So why `pinit` and not just one of the other approaches? Well...

### What about Yeoman?

I've always wanted to love [Yeoman](https://yeoman.io), and it's an excellent fit for many people. It's very JavaScripty, though, and I've had too many projects start with a failing `yo` generator. Not a great way to get started. Plus, many templates are close to what I might want but differ just enough to make the post-generation clean-up process annoying and not repeatable.

### Okay, `git init` templates!

"`git init` supports template directories," you say. "Why not use a series of those instead of creating a whole new thing?"

Read the [documentation on template directories](https://git-scm.com/docs/git-init#_template_directory), and you'll see the problem with `git init`. (Emphasis mine.)

> Files and directories in the template directory whose names **do not start with a dot** will be copied

How many *useful* template repositories would *not* contain file names beginning with a dot? :thinking:

### GitHub Repository Templates?

GitHub repository templates were [announced on June 6, 2019](https://github.blog/2019-06-06-generate-new-repositories-with-repository-templates/), and are [documented here](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-repository-from-a-template).

But:

* What if my new project isn't on GitHub?
* What if I don't know yet if I want a hosted remote?

### What if :scream: I'm not using git?

IKR? So crazy. :roll_eyes:

No, really: A whole bunch of folks use [Subversion](https://subversion.apache.org/). There's a thing called [Piper](https://cacm.acm.org/magazines/2016/7/204032-why-google-stores-billions-of-lines-of-code-in-a-single-repository/fulltext) that houses a gazillion lines of code. There's Perforce, Mercurial, and ... look at [this list](https://en.wikipedia.org/wiki/Comparison_of_version-control_software).
