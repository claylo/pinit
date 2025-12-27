# pinit comparisons

This document compares `pinit` to other ways of starting projects. It is not a formal debate club.
Use it to pick the right tool for your situation, then get back to shipping.

---

**Contents**

<!-- toc -->

* [1. Quick positioning](#1-quick-positioning)
* [2. Comparison table](#2-comparison-table)
* [3. Specific comparisons](#3-specific-comparisons)
  * [3.1 Yeoman](#31-yeoman)
  * [3.2 `git init` templates](#32-git-init-templates)
  * [3.3 GitHub Repository Templates](#33-github-repository-templates)
  * [3.4 Repository templates (GitLab, Gitea, Bitbucket, etc.)](#34-repository-templates-gitlab-gitea-bitbucket-etc)
  * [3.5 “Not using Git” (yes, it happens)](#35-not-using-git-yes-it-happens)
  * [3.6 Cookiecutter](#36-cookiecutter)
  * [3.7 Copier](#37-copier)
  * [3.8 cargo-generate](#38-cargo-generate)
  * [3.9 Framework CLIs (Rails, Cargo, npm, etc.)](#39-framework-clis-rails-cargo-npm-etc)
  * [3.10 Hygen / Plop / Slush (JS generator tools)](#310-hygen--plop--slush-js-generator-tools)
  * [3.11 Monorepo generators (Nx, Turborepo, etc.)](#311-monorepo-generators-nx-turborepo-etc)
  * [3.12 Dotfiles and bootstrap scripts](#312-dotfiles-and-bootstrap-scripts)
  * [3.13 Starter repos and “golden” repos](#313-starter-repos-and-golden-repos)
  * [3.14 Degit and template‑clone tools](#314-degit-and-template%E2%80%91clone-tools)
  * [3.15 IDE project templates](#315-ide-project-templates)
* [4. When pinit is the right tool](#4-when-pinit-is-the-right-tool)

<!-- tocstop -->

---

## 1. Quick positioning

`pinit` sits between “clone a template repo and fix it up” and “full code generator with prompts.”
It applies one or more template directories to an existing (or new) directory, and it can merge
safely when possible. It does not try to invent your architecture, your naming conventions, or your
future.

## 2. Comparison table

| Approach / tool | Strengths | Tradeoffs | When pinit is a good fit |
|---|---|---|---|
| Plain template repo clone | Simple, no tooling | Manual cleanup, no merge logic | When you want repeatable stacks or merges |
| `git init` templates | Built into Git | Skips dotfiles, limited control | When you need dotfiles and stackable templates |
| Repo templates (GitHub/GitLab/etc.) | One-click, integrates with hosting | Tied to host, not great for local-only | When you want local or non‑GitHub workflows |
| Yeoman | Mature ecosystem, interactive | JS‑heavy, generators can be brittle | When you want small, deterministic merges |
| Cookiecutter | Popular, language‑agnostic | Heavy prompting, one‑shot generation | When you want repeatable stacking and re‑apply |
| Copier | YAML config, updates supported | More moving parts, templating logic | When you want a minimal, merge‑first engine |
| cargo-generate | Rust‑friendly, git-based templates | Rust‑centric, mostly one‑shot | When you need cross‑language baseline templates |
| Framework CLIs (`cargo new`, `rails new`, etc.) | Opinionated, fast start | Locked to framework, not stackable | When you want to compose multiple templates |
| Hygen / Plop / Slush | Simple prompts, JS tooling | Typically generator‑driven, not merge‑aware | When you want non‑interactive merges |
| Monorepo generators (Nx, etc.) | Rich scaffolding | Heavy, domain‑specific | When you want small, reusable stacks |
| Dotfile/bootstrap scripts | Total control | DIY maintenance burden | When you want structured template reuse |
| Degit / template clone tools | Very fast, minimal setup | No merge logic, no stacking | When you want repeatable application and updates |
| IDE project templates | Convenient in-editor | IDE‑specific, not portable | When you want CLI‑driven, repo‑agnostic setup |

## 3. Specific comparisons

It’s not like I’m the first to say, “starting a new project should suck less.” So here’s the tour.

### 3.1 Yeoman

I’ve always wanted to love [Yeoman](https://yeoman.io). It’s an excellent fit for many people. It’s
also very JavaScripty, and I’ve had too many projects start with a failing `yo` generator. Not a
great way to get started. Plus, many templates are close to what I might want but differ just enough
to make the post‑generation clean‑up process annoying and not repeatable.

### 3.2 `git init` templates

“`git init` supports template directories,” you say. “Why not use a series of those instead of
creating a whole new thing?”

Read the [documentation on template directories](https://git-scm.com/docs/git-init#_template_directory),
and you’ll see the catch:

> Files and directories in the template directory whose names **do not start with a dot** will be copied

How many *useful* template repositories would *not* contain file names beginning with a dot?
:thinking:

### 3.3 GitHub Repository Templates

GitHub repository templates were [announced on June 6, 2019](https://github.blog/2019-06-06-generate-new-repositories-with-repository-templates/),
and are [documented here](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-repository-from-a-template).

But:

- What if my new project isn’t on GitHub?
- What if I don’t know yet if I want a hosted remote?

### 3.4 Repository templates (GitLab, Gitea, Bitbucket, etc.)

Repository templates are great when you’re all‑in on a hosting provider and happy to start every
project there. The moment you want a local‑only project, or a repo that lives somewhere else, the
magic fades. `pinit` works regardless of hosting, or even whether you use Git at all.

### 3.5 “Not using Git” (yes, it happens)

IKR? So crazy. :roll_eyes:

A whole bunch of folks use [Subversion](https://subversion.apache.org/). There’s Google’s
[Piper](https://cacm.acm.org/magazines/2016/7/204032-why-google-stores-billions-of-lines-of-code-in-a-single-repository/fulltext)
(a monorepo at scale). There’s [Perforce](https://www.perforce.com/), [Mercurial](https://www.mercurial-scm.org/),
and a long tail of version control systems. `pinit` doesn’t care. If you’re curious, there’s a
longer list at [Comparison of version control software](https://en.wikipedia.org/wiki/Comparison_of_version-control_software).

### 3.6 Cookiecutter

Cookiecutter is popular and flexible, but it is primarily a one‑shot generator with prompts and a
templating language. If you want to re‑apply a baseline later, or stack multiple templates without
tearing things apart, `pinit` tends to be calmer.

### 3.7 Copier

Copier brings YAML‑driven templates and supports updates. It’s a great tool when you want a
full template engine with variable substitution across files. If you want the smallest, safest merge
layer that can be applied repeatedly, `pinit` keeps the scope narrow on purpose.

### 3.8 cargo-generate

cargo-generate is excellent for Rust templates that live in git repos. If you’re starting a Rust
project and want a rich template experience, it’s a strong choice. `pinit` is more about composing
multiple templates across languages and re‑applying baselines later.

### 3.9 Framework CLIs (Rails, Cargo, npm, etc.)

Framework generators are fantastic for their ecosystems. They’re also usually fixed to that
framework’s worldview. `pinit` is for stitching together *your* baseline across languages and toolchains.

### 3.10 Hygen / Plop / Slush (JS generator tools)

These are lightweight and useful for prompt‑driven scaffolding. If you want to apply templates on
top of an existing project and merge safely, `pinit` is more of a patch tool than a wizard.

### 3.11 Monorepo generators (Nx, Turborepo, etc.)

Monorepo tools can generate a lot quickly, but they come with their own ecosystem and assumptions.
`pinit` is intentionally smaller and better suited to “add a baseline to this repo” than “manage a
fleet of apps.”

### 3.12 Dotfiles and bootstrap scripts

You can absolutely do this yourself with shell scripts and a heroic amount of discipline. The
tradeoff is maintenance overhead and zero merge logic. `pinit` gives you repeatable, stackable
baselines without a single `sed -i` in sight.

### 3.13 Starter repos and “golden” repos

A single “starter repo” works well until it doesn’t. It’s easy to drift, hard to compose, and
annoying to update after the fact. `pinit` lets you split the baseline into reusable pieces and
apply them where needed.

### 3.14 Degit and template‑clone tools

Tools like `degit` (or a plain `git clone` + delete history) are great when you want speed and
don’t need re‑application. If you want to apply the same baseline to *existing* projects, or stack
multiple templates, `pinit` is the calmer option.

### 3.15 IDE project templates

Many IDEs ship project templates. They’re convenient and fast, but they’re tied to the editor and
not easily shared across teams. `pinit` is just a CLI, so it works no matter which editor you’ve
chosen to argue with this week.

## 4. When pinit is the right tool

Use `pinit` when you want:

- Multiple templates applied in a specific order.
- A repeatable baseline that can be re‑applied later.
- Safe, additive merges for common file types.
- A tool that doesn’t require a specific VCS or hosting provider.

If you want a full scaffolding wizard with dozens of interactive prompts, you probably want a
generator. If you want a steady baseline that doesn’t get in your way, `pinit` does the boring parts
and lets you do the interesting ones.
