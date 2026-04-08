# DSL Reference

## File structure

Each `.tcon` entry should export:

- `spec`
- `schema`
- `config`

## `spec`

```ts
export const spec = {
  path: "server.json",
  format: "json",
  mode: "replace",
};
```

- `path`: output file path (relative to workspace root)
- `format`: `json | yaml | env | toml | properties`
- `mode`: currently `replace`

## `schema` roots

- `t.string()`
- `t.number()`
- `t.boolean()` / `t.bool()`
- `t.object({...})`
- `t.array(schema)`
- `t.record(schema)` (object with arbitrary keys and uniform value schema)
- `t.enum(["a","b"])`
- `t.union([schemaA, schemaB, ...])`
- `t.literal("x")` / `t.literal(1)` / `t.literal(true)` / `t.literal(null)`

## Supported schema methods

- `.default(value)`
- `.optional()`
- `.min(n)`
- `.max(n)`
- `.int()`
- `.strict()` — unknown keys in `config` for that object are **not** copied into the compiled output (they are ignored). Validation does not fail solely because of extra keys; use review/drift checks if you need to catch them.

## Imports

```ts
import { sharedSchema, sharedConfig } from "./base.tcon";
```

- Named imports only
- Cycle-safe resolution

## Unsupported language features

- functions, loops, conditionals
- spread/computed properties
- arbitrary runtime expression execution
