# CI/CD integration

`tcon` is designed for **validate → check → (optional) build** in automation: compile schemas and config in CI without surprises, then ensure committed artifacts match sources.

## Commands

| Goal | Command | Writes files | Typical use |
|------|---------|--------------|-------------|
| Typecheck / compile only | `tcon validate` | No | PR gate, fast feedback |
| Regenerate committed outputs | `tcon build` or `tcon generate` | Yes | Release or local refresh |
| Drift gate (sources vs disk) | `tcon check` | No | CI “golden file” enforcement |
| Human-readable diff | `tcon diff` | No | Local debugging |
| Live rebuild | `tcon watch` | Yes | Local development |

Recommended CI sequence when generated files are committed:

```bash
tcon validate
tcon check
tcon secrets
```

If you only want to ensure `.tcon` compiles (no committed outputs yet), use:

```bash
tcon validate
```

## GitHub Actions (Rust / `cargo`)

```yaml
name: tcon

on:
  push:
    branches: [main, master]
  pull_request:
  workflow_dispatch:

jobs:
  config:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - name: Install tcon CLI
        run: cargo install --path . --locked

      - name: Validate .tcon (compile, no writes)
        run: tcon validate

      - name: Drift check (optional — when outputs are committed)
        run: tcon check

      - name: Secrets audit
        run: tcon secrets
```

To run **without** installing the binary (slower but zero install step):

```yaml
      - name: Validate
        run: cargo run -- validate

      - name: Check drift
        run: cargo run -- check
```

## JSON diagnostics in CI

For logs or PR comments driven by tooling:

```bash
tcon --error-format json validate
```

Each failure prints one JSON object per line on stderr with stable `code` values (see `docs/diagnostics/v1.md`).

## Live watch locally

Not usually run in CI. For local workflows:

```bash
tcon watch
tcon watch --entry api/server.tcon --interval-ms 500
```

`--interval-ms` sets the polling interval (minimum `100`). Stop with Ctrl+C.

## Cache keys

If you only change `.tcon` sources rarely, you can split jobs: run `tcon validate` on every PR and `tcon check` only when `compat/**` or generator logic changes—tune to your repo.
