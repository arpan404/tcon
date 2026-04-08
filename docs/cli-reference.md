# CLI Reference

## Global flags

- `--error-format text|json`
  - `text`: human-readable diagnostics
  - `json`: machine-readable diagnostics (`code`, `message`, `file`, `line`, `col`)

`--help` uses ANSI colors when stderr is a TTY. Set `NO_COLOR=1` (or a non-empty value) to force plain text.

## `validate` vs `check` (important)

| Command | Reads `.tcon` | Reads `spec.path` on disk | Writes outputs |
|--------|-------------|---------------------------|----------------|
| `validate` | yes | **no** | **no** |
| `check` | yes | **yes** (compares) | no |
| `build` | yes | only to overwrite | **yes** |

`tcon validate` answers: **do my `.tcon` sources compile and does `config` match `schema`?**  
It does **not** open your existing JSON/YAML/etc. artifact. If someone hand-edited `server.json` to invalid types, **`validate` still exits 0** as long as `.tcon` is fine.

`tcon check` answers: **does the file on disk match what the compiler would produce?**  
Use it in CI when generated files are committed. Typical sequence: `tcon validate && tcon check`.

Unknown subcommands that look like typos (e.g. `checl`) print `did you mean 'check'?`.

## Commands

- `tcon validate [--entry <file.tcon>]`
  - Run the full compile pipeline (parse, schema defaults, `config` validation, emit in memory) **without** reading or writing `spec.path` files. Prints where artifacts *would* go. Use in CI for “sources compile” gates.
- `tcon build [--entry <file.tcon>]`
  - Compile one or all entries and write outputs to `spec.path`.
- `tcon generate [--entry <file.tcon>]`
  - Alias of `build` (same flags and behavior).
- `tcon check [--entry <file.tcon>]`
  - Recompute expected output and fail on drift vs on-disk files.
- `tcon diff [--entry <file.tcon>]`
  - Print compact unified drift hunks.
- `tcon print --entry <file.tcon>`
  - Print unresolved parsed program for debugging. Also shows evaluated config with **secret fields redacted** as `"[secret]"`.
- `tcon watch [--entry <file.tcon>] [--interval-ms <n>]`
  - Poll the entry file and transitive imports; rebuild on change. Default poll interval `800` ms; minimum `100`. Lists watched paths on startup; stop with Ctrl+C.
- `tcon init [--preset <name>] [--force]`
  - Scaffold starter `.tcon` templates.
  - Presets: `json`, `yaml`, `env`, `toml`, `properties`, `all`.
- `tcon secrets`
  - Audit the current git repository for exposed secrets. Checks git-tracked files and the staging area for risky file names (`.env*`, `*.key`, `*.pem`, `credentials.json`, etc.) and secret-like content (private key blocks, token/password-like assignments). Also reports common secret patterns not covered by git ignore rules.
  - Exits non-zero if any tracked/staged secret exposure is detected.
- `tcon --help`
- `tcon --version`

## Exit codes

- `0`: success
- non-zero: compile/validation/drift/IO error

## CI

See `docs/ci.md` for GitHub Actions and recommended `validate` / `check` workflows.

## Secret management

### Marking schema fields as secret

Use the `.secret()` modifier to mark a **string schema field** as sensitive:

```ts
export const schema = t.object({
  host: t.string().default("localhost"),
  port: t.number().int().default(5432),
  password: t.string().secret(),          // must come from ${ENV_VAR}
  api_key:  t.string().secret().optional(),
}).strict();
```

**Enforcement**: `tcon validate` and `tcon build` will error if a field marked `.secret()` has a hardcoded literal value in `config`. This is enforced recursively (nested objects, arrays, records, and matching union variants). Secret fields must be sourced via environment variable interpolation:

```ts
// ✗ error: password is secret — must use ${VAR} interpolation
export const config = {
  password: "hunter2",
};

// ✓ correct: resolved from environment at build time
export const config = {
  password: "${DB_PASSWORD}",
};
```

`t.string().secret()` fields must not declare `.default(...)`; provide the value from env-backed `config` instead.

**Redaction in `print`**: `tcon print` shows evaluated config with secret fields replaced by `"[secret]"` so no plaintext secrets appear in debug output.

### Environment variable interpolation

All string config values support `${VAR_NAME}` and `${VAR_NAME:default}` syntax:

```ts
export const config = {
  host:     "${DB_HOST:localhost}",   // fallback to "localhost" if unset
  password: "${DB_PASSWORD}",         // fails at build time if unset
};
```

### Auditing git exposure

Run `tcon secrets` to check whether secret files are accidentally tracked by git or missing from `.gitignore`:

```
$ tcon secrets
tcon secrets audit
✗  1 secret file(s) are tracked by git (CRITICAL):
   .env  (environment variable file)

  Fix: Remove from tracking with:
    git rm --cached .env
  Then add to .gitignore and commit the removal.

⚠  3 common secret pattern(s) not in .gitignore:
   .env.*        # environment variable file
   *.pem         # PEM certificate/key
   *.key         # private key
```

Add `tcon secrets` to your CI pipeline:

```yaml
- name: Secrets audit
  run: tcon secrets
```
