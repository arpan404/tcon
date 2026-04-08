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

Only these keys are allowed; extra keys are rejected.

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
- `.strict()` — like Zod’s `strict()`, unknown keys in `config` for that object are a **validation error** (the build fails with a list of keys).
- `.extend(objectSchema)` — merge fields from another object schema; local fields win on conflicts.
- `.secret()` — marks a **string field** as sensitive. The value **must** use `${ENV_VAR}` interpolation; hardcoded literals are rejected at validation time. Redacted as `"[secret]"` in `tcon print` debug output.

For safety, secret string fields must not declare `.default(...)`; provide secret values through env-backed `config` values instead.

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
