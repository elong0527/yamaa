# Locked Cargo builds

`rust/Cargo.lock` pins one dependency resolution for the shared engine and both
bindings. Python and R source packages retain that lock, the root workspace
manifest and the Rust toolchain declaration byte for byte. Tests, probes and
release builds use `--locked`, so a missing or stale lock fails instead of
resolving a new graph during qualification.
This implements the packaging decision in
[#1906](https://github.com/elong0527/yamaa/issues/1906); installed Windows R
qualification remains required under
[#1903](https://github.com/elong0527/yamaa/issues/1903).

## Python source workspace

Pinned maturin 1.9.6 copies the full workspace lock into an sdist but rewrites the
root workspace members to the Python binding and its reachable path dependencies.
That removes the R member while retaining its lock entries. A locked build of
that real source archive rejects the resulting workspace and requests a lock
update.

`python/yamaa_build_backend.py` delegates Python wheel, editable and metadata
hooks to maturin. Its sdist hook restores the original root workspace manifest
and includes missing workspace member sources after maturin creates the archive.
Restoration admits a member's `Cargo.toml`, optional `build.rs` and
`src/**/*.rs` library sources. Local notes, generated files and symlinks are
excluded or refused. Additional resources require a reviewed source-membership
policy update.
The lock remains unchanged. The backend refuses missing or contradictory locks,
missing manifests, invalid archive roots and duplicate archive entries before
replacing the archive.
Python still builds only `yamaa-python`; including the R sources does not add an
R dependency to that binary or require R to build it.

Use a PEP 517 frontend for source distributions:

```sh
uv build --wheel --sdist --out-dir /tmp/yamaa-dist python
```

Direct `maturin sdist` and `maturin build --sdist` bypass the preservation hook.
CI builds the direct wheel with `maturin build --release --locked`, then invokes
the same sdist hook through `python yamaa_build_backend.py OUTPUT_DIRECTORY`.
The backend itself is included in the source archive for subsequent frontend
builds.

## R source workspace

`rust/tools/stage_r_package.py` requires the canonical lock before creating a
destination. It copies the root manifest, toolchain declaration and lock into
`src/rust`, together with all five crates. It excludes generated targets and
per-crate lockfiles. `R/yamaa/src/Makevars` selects `yamaa-r` and uses `--locked`
for the installed source-package build.

## Build evidence

`rust/tools/check_cargo_lock.py` runs full `cargo metadata --locked`, including
transitive dependencies. For each supplied Python or R source archive it checks
that exactly one regular Cargo lock appears at the expected workspace location,
and compares its bytes, the root workspace manifest and Rust toolchain declaration
against the checkout.

```sh
python rust/tools/check_cargo_lock.py \
  --output-dir /tmp/yamaa-dependencies \
  /tmp/yamaa-dist/yamaa-0.3.0.tar.gz /tmp/yamaa-dist/yamaa_0.3.0.tar.gz
```

The output directory retains complete Cargo metadata, the all-target feature
tree, Cargo version and `rustc -vV`. Native Rust, Python distribution and R
distribution CI lanes upload these records. Installed wheel/source suites and
strict R package checks qualify the resulting packages separately; dependency
metadata alone does not qualify execution or a platform.

Dependency changes require a reviewed canonical lock update, rerun boundary
checks and fresh package qualification. Source-package staging must never resolve
or rewrite the lock. The narrow packaging exception in `AGENTS.md` admits this
Cargo lock and unchanged staged copies alongside `python/uv.lock`; runtime,
contract, activation-cache and resource-snapshot digest identities remain
prohibited.
