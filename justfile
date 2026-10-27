# Builds this service standalone from its own repository (tacplus.yaml: floor
# and depends). The looper runs `just gates`.

TACPLUS := env_var_or_default("TACPLUS_BIN", "tacplus")

default: generate

generate:
  #!/usr/bin/env bash
  set -euo pipefail
  {{TACPLUS}} build --tag latest

# What every change (ports, releases) must pass: regenerate with the floor's
# compiler (generated code lives only in target/, so generation touching any
# file git sees is a failure), then build and test.
gates:
  #!/usr/bin/env bash
  set -euo pipefail
  tracked="$(git ls-files target)"
  test -z "$tracked" || { echo "gates: generated files are tracked:" >&2; echo "$tracked" >&2; exit 1; }
  before="$(git status --porcelain)"
  {{TACPLUS}} build --tag latest
  if [[ "$(git status --porcelain)" != "$before" ]]; then
    echo "gates: generation changed files outside target/:" >&2
    git status --porcelain >&2
    exit 1
  fi
  cargo build --locked --all-targets
  cargo test --locked
  echo "gates: ok"
