# GitHub Publication Guide

This project publishes release binaries through GitHub Releases.

Workflows are intentionally disabled for auto-runs right now, so publishing is manual.

## 1) Build release artifacts

On macOS/Linux:

```bash
./scripts/package-release.sh 1.0.0
```

On Windows PowerShell:

```powershell
./scripts/package-release.ps1 -Version 1.0.0
```

Artifacts are written to `dist/`.

## 2) Create git tag

```bash
git tag v1.0.0
git push origin v1.0.0
```

## 3) Publish GitHub release with assets

```bash
gh release create v1.0.0 dist/* \
  --title "tcon v1.0.0" \
  --notes-file RELEASE.md
```

## 4) Verify install paths

- macOS/Linux local install: `./scripts/install.sh`
- Windows local install: `./scripts/install.ps1`
