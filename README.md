# tcon

`tcon` is a zero-dependency typed configuration compiler in Rust.

It compiles `.tcon` files (TypeScript-like DSL) into deterministic config outputs and detects drift.

Documentation:

- User guide: `docs/user-guide.md`
- CLI reference: `docs/cli-reference.md`
- DSL reference: `docs/dsl-reference.md`
- Diagnostics contract: `docs/diagnostics/v1.md`
- Publication guide: `docs/publication/github.md`
- CI/CD: `docs/ci.md`

## Project maturity

`tcon` is currently versioned as `v1.0.0` and includes frozen compatibility snapshots in `compat/v1/`.

## Install CLI

Build and install locally:

```bash
./scripts/install.sh
```

On Windows PowerShell:

```powershell
./scripts/install.ps1
```

Then run directly:

```bash
tcon --help
tcon --version
```

Uninstall:

- macOS/Linux: `./scripts/uninstall.sh`
- Windows: `./scripts/uninstall.ps1`

## Commands

- `tcon build [--entry <file.tcon>]` - generate output files
- `tcon check [--entry <file.tcon>]` - verify generated files are up-to-date
- `tcon diff [--entry <file.tcon>]` - show first-difference drift summaries
- `tcon print --entry <file.tcon>` - print parsed AST/program
- `tcon watch [--entry <file.tcon>]` - rebuild when entry files change
- `tcon init [--preset <name>] [--force]` - scaffold `.tcon` entries for common formats

Global flags:

- `--error-format text|json` - emit human-readable or machine-readable structured errors

## Supported output formats

- `json`
- `yaml`
- `env`
- `toml`
- `properties`

## Safety guarantees

- `spec.path` must stay inside the workspace (absolute paths, `..` traversal, and symlink escapes are rejected).
- `.secret()` fields must be environment-sourced (`${VAR}` / `${VAR:default}`) and are redacted in `tcon print`.
- `.secret()` fields do not permit `.default(...)` to avoid embedding fallback secrets in schema definitions.

## Minimal `.tcon` example

```ts
export const spec = {
  path: "server.json",
  format: "json",
  mode: "replace",
};

export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().min(1).max(65535).default(8080),
}).strict();

export const config = {
  port: 3000,
};
```

## Imports

Import symbols from other `.tcon` files:

```ts
import { sharedSchema, sharedConfig } from "./base.tcon";
export const schema = sharedSchema;
export const config = sharedConfig;
```

## Local quality gates

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

## Release packaging

Generate release archives + checksums:

- macOS/Linux: `./scripts/package-release.sh 1.0.0 [targets...]`
- Windows: `./scripts/package-release.ps1 -Version 1.0.0 [-Target ...]`