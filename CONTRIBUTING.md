# Contributing to tcon

## Current Stability Policy

`tcon` is currently at `v1.0.0`. Schema/CLI behavior is SemVer-governed, and compatibility is tracked via `compat/v1/` snapshots and diagnostics contracts under `docs/diagnostics/`.

## Development Quality Gates

Every change should pass:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

## Change Guidelines

- **`.strict()`** objects fail the build if `config` contains keys not listed on that object (Zod-style), rather than dropping them.
- **`.default(...)`** values on the schema tree are validated (type, min/max, strict keys, etc.) before `config` is processed.
- **`spec`** objects must not contain keys other than `path`, `format`, and `mode`.
- Keep deterministic output guarantees intact.
- Avoid external dependencies unless explicitly approved.
- Add tests for any new DSL surface or validation behavior.
- Prefer improving diagnostics over silent coercions.
- Keep import and watch behavior cycle-safe and deterministic.

## Commit Strategy

- Commit in small, reviewable slices (parser/eval/validator/CLI/docs).
- Use messages focused on intent and impact.
- Include migration notes in commit body when behavior changes.
