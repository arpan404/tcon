# CLI Reference

## Global flags

- `--error-format text|json`
  - `text`: human-readable diagnostics
  - `json`: machine-readable diagnostics (`code`, `message`, `file`, `line`, `col`)

`--help` uses ANSI colors when stderr is a TTY. Set `NO_COLOR=1` (or a non-empty value) to force plain text.

## Commands

- `tcon validate [--entry <file.tcon>]`
  - Run the full compile pipeline (parse, schema defaults, `config` validation, emit in memory) **without** writing `spec.path` files. Use in CI for fast “does it compile?” gates.
- `tcon build [--entry <file.tcon>]`
  - Compile one or all entries and write outputs to `spec.path`.
- `tcon generate [--entry <file.tcon>]`
  - Alias of `build` (same flags and behavior).
- `tcon check [--entry <file.tcon>]`
  - Recompute expected output and fail on drift vs on-disk files.
- `tcon diff [--entry <file.tcon>]`
  - Print compact unified drift hunks.
- `tcon print --entry <file.tcon>`
  - Print unresolved parsed program for debugging.
- `tcon watch [--entry <file.tcon>] [--interval-ms <n>]`
  - Poll the entry file and transitive imports; rebuild on change. Default poll interval `800` ms; minimum `100`. Lists watched paths on startup; stop with Ctrl+C.
- `tcon init [--preset <name>] [--force]`
  - Scaffold starter `.tcon` templates.
  - Presets: `json`, `yaml`, `env`, `toml`, `properties`, `all`.
- `tcon --help`
- `tcon --version`

## Exit codes

- `0`: success
- non-zero: compile/validation/drift/IO error

## CI

See `docs/ci.md` for GitHub Actions and recommended `validate` / `check` workflows.
