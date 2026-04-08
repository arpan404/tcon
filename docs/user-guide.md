# tcon User Guide

## What tcon does

`tcon` compiles typed `.tcon` source files into deterministic configuration artifacts and helps detect configuration drift.

## Quickstart

1. Install CLI
   - macOS/Linux: `./scripts/install.sh`
   - Windows PowerShell: `./scripts/install.ps1`
2. Initialize starter templates:
   - `tcon init`
3. Validate `.tcon` sources only (does not read generated files on disk):
   - `tcon validate` — use `tcon check` to catch stale or hand-edited outputs
4. Build generated configs:
   - `tcon build` (or `tcon generate`)
5. Verify drift in CI/local:
   - `tcon check`
6. Live reload while editing:
   - `tcon watch` (optional: `--interval-ms 500`)

## Typical workflow

- Author `.tcon` files under `.tcon/`.
- Commit generated output files (or generate in a release step).
- In CI: run `tcon validate` (fast compile-only) and/or `tcon check` (drift vs repo). See `docs/ci.md`.

## Example

```ts
export const spec = { path: "server.json", format: "json", mode: "replace" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();
export const config = { port: 3000 };
```

`.strict()` objects reject any `config` keys that are not declared on the schema. Every `.default(...)` is checked against its field’s type before `config` is validated (`spec` only allows `path`, `format`, and `mode`).

## More docs

- CLI reference: `docs/cli-reference.md`
- CI/CD: `docs/ci.md`
- DSL reference: `docs/dsl-reference.md`
- Diagnostics JSON codes: `docs/diagnostics/v1.md`
- GitHub publication: `docs/publication/github.md`
