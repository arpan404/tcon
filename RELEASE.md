# Release Checklist

## Pre-release

- Update `README.md` and `AGENTS.md` if behavior changed.
- Ensure CI is green on `master`.
- Run locally:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-targets`

## Compatibility Review

- Confirm output determinism did not regress.
- Confirm drift checks still produce stable, actionable output.
- Document any DSL breaking changes.

## Tagging and publication

- Bump crate version in `Cargo.toml`.
- Create git tag: `vX.Y.Z`.
- Publish release notes with:
  - New features
  - Breaking changes
  - Migration notes
  - Validation/diagnostics improvements

## Artifacts

- Build archives and checksums:
  - macOS/Linux: `./scripts/package-release.sh X.Y.Z [targets...]`
  - Windows: `./scripts/package-release.ps1 -Version X.Y.Z [-Target ...]`
- Publish to GitHub Releases:
  - `gh release create vX.Y.Z dist/* --title "tcon vX.Y.Z" --notes-file RELEASE.md`
