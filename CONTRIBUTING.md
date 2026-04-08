# Contributing to tcon

## Current Stability Policy

`tcon` is in a pre-v1 hardening phase. Breaking changes are allowed while we finalize the language and runtime contract. Once v1 is tagged, schema/CLI behavior will be SemVer-governed.

## Development Quality Gates

Every change should pass:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

## Change Guidelines

- Keep deterministic output guarantees intact.
- Avoid external dependencies unless explicitly approved.
- Add tests for any new DSL surface or validation behavior.
- Prefer improving diagnostics over silent coercions.
- Keep import and watch behavior cycle-safe and deterministic.

## Commit Strategy

- Commit in small, reviewable slices (parser/eval/validator/CLI/docs).
- Use messages focused on intent and impact.
- Include migration notes in commit body when behavior changes.
