# Development

yamaa uses one shared Rust engine for the Python and R packages. Start with
[the language rules](articles/rules.md), then the
[schema sources](https://github.com/elong0527/yamaa/tree/main/yaml) and
[benchmarks](https://github.com/elong0527/yamaa/tree/main/benchmarks). Rules define
behavior; independently authored expected files define executable test truth.
Keep those files unchanged when fixing an implementation or moving tests.

## Where changes belong

| Area | Source and responsibility |
| --- | --- |
| Language | `rules/`, `yaml/`: normative contracts, diagnostic registry and schemas. |
| Shared semantics | `rust/crates/yamaa-core/`: values, expressions, admission and compilation. |
| Shared lifecycle | `rust/crates/yamaa-engine/`: execution, resource/function ports, verification and publication. |
| Storage and formats | `rust/crates/yamaa-adapters/`: YAML, CSV, Parquet, files, project environments and submission rendering. |
| Host boundaries | `rust/crates/yamaa-python/`, `rust/crates/yamaa-r/`: arguments, callbacks, scalar conversions and owned result views. |
| Host packages | `python/`, `R/yamaa/`: thin public facades, private style/ODM helpers and package builds. |
| Installed qualification | `rust/tests/`, `R/yamaa/tests/`, `rust/qualification/`: installed contracts, independent fixtures and required coverage. |
| Automation | `.github/workflows/`, `.github/scripts/`, `rust/tools/`: CI selection, staging, comparisons and evidence replay. |

Core and engine remain `no_std` and exclude host/storage frameworks. Diagnostics
belong to the shared registry. Host callbacks carry typed values; diagnostic
JSON text does not introduce a callback request/reply protocol. Build computes
and retains results; explicit Save publishes held bytes in the contract's order.

Rust module paths follow their directories. Core `dataset/`, `diagnostic/` and
`specification/` own their related child modules; engine `dataset/` owns execution
and intermediate helpers; adapters group schema data, specification sources,
diagnostics and define publication controls under their matching parents.
Start at each family's `mod.rs` and follow its ordinary `mod` declarations.
The sole production `#[path]` exception selects the Windows implementation of
`file_publication` while preserving its existing public and private namespace.

The [Python guide](https://github.com/elong0527/yamaa/blob/main/python/README.md)
and [R guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md)
describe public usage and installation. The
[Rust entry point](https://github.com/elong0527/yamaa/blob/main/rust/README.md)
describes crate checks. [Release guidance](release.md) owns migration decisions
and pending acceptance.

## Local checks

Read [AGENTS.md](https://github.com/elong0527/yamaa/blob/main/AGENTS.md) before
editing. Run `make help` at the repository root to discover supported targets,
their prerequisites and configuration variables. The root
[Makefile](https://github.com/elong0527/yamaa/blob/main/Makefile) calls the same
owned scripts as full repository and native CI. GNU make and Python 3.12 or
later are sufficient to read the help inventory; individual checks need the
tools listed there.

Use Rust 1.90.0 and the canonical `rust/Cargo.lock`. Python and R source archives
stage the unchanged workspace and lock. From the repository root:

```sh
make help
make sync-python
make check-fast
make check-repository
make test-rust
```

`sync-python` requires uv 0.11.6 and installs the locked development test
environment. The other targets keep repository feedback, shared Rust checks,
installed-host checks and release qualification separate:

| Target | Checks and context |
| --- | --- |
| `check-fast` | Python formatting/lint and CI selector/command controls in the synced development environment. |
| `check-repository` | Validator controls, available-host repository validation, style, inventory, documentation and generated rule/reference guards. R semantic coverage still requires installed R CI. |
| `check-blockers` | Live validation-blocker issues; requires GitHub API access and its token. |
| `test-rust` | Locked shared-workspace debug tests, engine `test-support`, then optimized `release-test` tests using the pinned Rust toolchain. |
| `stage-python`, `test-python` | Stage the cataloged supplemental suites and run them through a separately installed noneditable Python package outside the checkout. |
| `stage-r`, `stage-r-tests`, `test-r`, `check-r` | Stage the standalone source and cataloged R suites; run installed supplemental checks or a fresh package check requiring `Status: OK`. |
| `docs-test`, `docs-generate`, `docs-build`, `docs` | Canonical-page tests, dashboard generation and strict site build with the pinned documentation dependencies. |
| `audit-native`, `audit-replay` | Independent exact-run fetch or offline retained-input replay, both requiring the auditor's `--check` verdict. |

For boundary/API guards and focused component checks beyond these targets:

```sh
uv run --project python --no-sync python rust/tools/check_release_api.py
uv run --project python --no-sync python rust/tools/check_shared_release_versions.py
uv run --project python --no-sync python rust/tools/check_dependencies.py
uv run --project python --no-sync python -m unittest discover -s rust/tests -p 'test_*.py'
```

Rust formatting and lint checks run from `rust/`:

```sh
cargo fmt --all --check
cargo clippy --locked -p yamaa-core -p yamaa-engine -p yamaa-adapters -p yamaa-python --all-targets -- -D warnings
```

The engine feature check in `test-rust` includes integration binaries omitted
by a default test invocation. Portable math has shared authored golden values in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`; run
`cargo test --locked -p yamaa-core --test portable_math_golden` from `rust/`.
Installed supplemental suites require yamaa and `yamaa-benchmarks` installed
with locked test dependencies in a separate Python interpreter, or yamaa and
`yamaabenchmarks` in the R library selected by `R_LIBS`. `sync-python` prepares
the development environment; installed-host checks verify distribution ownership
and refuse checkout execution. Their commands and comparison rules are in
[qualification](https://github.com/elong0527/yamaa/blob/main/rust/qualification/README.md).

For Python, select the staging interpreter and installed host explicitly:

```sh
make stage-python PYTHON=/path/to/build-env/bin/python HOST_WORK=/tmp/yamaa-python-checks
make test-python HOST_PYTHON=/path/to/installed-env/bin/python \
  HOST_WORK=/tmp/yamaa-python-checks OUTPUT=/tmp/yamaa-python-reports \
  PACKAGE_FORM=wheel ARTIFACT_REFERENCE=/path/to/installed.whl \
  HOST_ARTIFACT_REFERENCE=/path/to/reference.whl EVIDENCE=/path/to/actual-run
```

Replace the example paths with the actual installed artifacts and run reference.
`SOURCE_REVISION` defaults to the checkout's HEAD and must match the tested
source. `HOST_WORK` must be outside the checkout; output directories must be
fresh to preserve previous evidence. `check-r` also requires a fresh `HOST_WORK`
directory that does not already exist. For R source staging and package checks:

```sh
make stage-r PYTHON=/path/to/build-env/bin/python R_STAGE=/tmp/yamaa-r-stage/yamaa
R_LIBS=/path/to/installed-r-library make check-r \
  R_ARCHIVE=/path/to/yamaa_0.4.0.tar.gz HOST_WORK=/tmp/yamaa-r-check
```

Build/install the staged source, its project program and dependencies first,
following the R package guide. `stage-r-tests` uses an external `HOST_WORK`;
`test-r` additionally takes fresh `OUTPUT`, actual `ARTIFACT_REFERENCE` and
`EVIDENCE`. Python and R supplemental tests do not replace public benchmark
conformance or the complete installed native matrix defined in CI guidance.
Expected benchmark files and Parquet fixtures are independent truth; do not
regenerate them to accommodate implementation changes.

Normative rule checks derive current REQ owners from `rules/` and schema
navigation from `yaml/`. The generated requirement and schema-field indexes follow
those sources. Independent owner expectations live in
[schema-navigation.tsv](https://github.com/elong0527/yamaa/blob/main/.github/scripts/yaml-validation/fixtures/schema-navigation.tsv);
`test_rule_rewrite.py` checks all 363 owners without regenerating this fixture.
Run from the repository root:

```sh
uv run --project python --no-sync python .github/scripts/commands/repository.py rule-references
uv run --project python --no-sync python .github/scripts/commands/repository.py rules
```

## Documentation and CI

Edit the root `README.md` for the site home and `rules/README.md` for its rules
index. The MkDocs hook reads those canonical sources; no copied pages exist.
Other site pages live in `docs/`. The
[documentation command guide](https://github.com/elong0527/yamaa/blob/main/.github/scripts/docs/README.md)
owns dependency setup, interpreter selection and the shared Make targets for
tests and builds. Deployment also renders the Quarto decks.

[CI guidance](https://github.com/elong0527/yamaa/blob/main/.github/ci.md)
defines the scoped Linux PR smoke checks and full native qualification route.
PR feedback does not establish complete installed or release qualification. [Release guidance](release.md)
separates merged implementation, qualified revisions, pending decisions and
publication. Keep current guidance here and there; retain historical plans in
Git and put new evidence in the relevant PR/issue and qualification records.
