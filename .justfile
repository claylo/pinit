# pinit justfile

set shell := ["zsh", "-c"]
set dotenv-load := true

default:
  @just --list

fmt:
  cargo fmt --all

clippy:
  cargo clippy --all-targets --all-features -- -D warnings

test:
  cargo nextest run

test-ci:
  cargo nextest run --profile ci

doc-test:
  cargo test --doc

cov:
  cargo llvm-cov nextest --html

check: fmt clippy test doc-test

