# v1 Compatibility Snapshot

This directory is a frozen compatibility snapshot for pre-v1 stabilization.

- `success/` contains cases that must build and match expected outputs exactly.
- `failure/` contains cases that must fail with expected machine-readable error code and message snippet.

The test suite (`tests/compat_matrix.rs`) executes these fixtures directly and should fail on behavioral drift.
