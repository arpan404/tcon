# tcon

`tcon` is a zero-dependency typed configuration compiler in Rust.

It compiles `.tcon` files (TypeScript-like DSL) into deterministic config outputs and detects drift.

## Project maturity

`tcon` is in a pre-v1 production-hardening phase. Breaking changes are still allowed while we lock the v1 DSL/runtime contract.

## Commands

- `tcon build [--entry <file.tcon>]` - generate output files
- `tcon check [--entry <file.tcon>]` - verify generated files are up-to-date
- `tcon diff [--entry <file.tcon>]` - show first-difference drift summaries
- `tcon print --entry <file.tcon>` - print parsed AST/program
- `tcon watch [--entry <file.tcon>]` - rebuild when entry files change

## Supported output formats

- `json`
- `yaml`
- `env`

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