# tcon User Guide

## What tcon does

`tcon` compiles typed `.tcon` source files into deterministic configuration artifacts and helps detect configuration drift.

## Quickstart

1. Install CLI
   - macOS/Linux: `./scripts/install.sh`
   - Windows PowerShell: `./scripts/install.ps1`
2. Initialize starter templates:
   - `tcon init`
3. Build generated configs:
   - `tcon build`
4. Verify drift in CI/local:
   - `tcon check`

## Typical workflow

- Author `.tcon` files under `.tcon/`.
- Commit generated output files.
- Run `tcon check` in CI to block drift.

## Example

```ts
export const spec = { path: "server.json", format: "json", mode: "replace" };
export const schema = t.object({
  host: t.string().default("0.0.0.0"),
  port: t.number().int().default(8080),
}).strict();
export const config = { port: 3000 };
```

## More docs

- CLI reference: `docs/cli-reference.md`
- DSL reference: `docs/dsl-reference.md`
- Diagnostics JSON codes: `docs/diagnostics/v1.md`
- GitHub publication: `docs/publication/github.md`
