# CLI Reference

## Global flags

- `--error-format text|json`
  - `text`: human-readable diagnostics
  - `json`: machine-readable diagnostics (`code`, `message`, `file`, `line`, `col`)

## Commands

- `tcon build [--entry <file.tcon>]`
  - Compile one or all entries and write outputs.
- `tcon check [--entry <file.tcon>]`
  - Recompute expected output and fail on drift.
- `tcon diff [--entry <file.tcon>]`
  - Print compact unified drift hunks.
- `tcon print --entry <file.tcon>`
  - Print unresolved parsed program for debugging.
- `tcon watch [--entry <file.tcon>]`
  - Watch entry + transitive imports and rebuild on change.
- `tcon init [--preset <name>] [--force]`
  - Scaffold starter `.tcon` templates.
  - Presets: `json`, `yaml`, `env`, `toml`, `properties`, `all`.
- `tcon --help`
- `tcon --version`

## Exit codes

- `0`: success
- non-zero: compile/validation/drift/IO error
