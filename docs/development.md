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
The [Python guide](https://github.com/elong0527/yamaa/blob/main/python/README.md)
and [R guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md)
describe public usage and installation. The
[Rust entry point](https://github.com/elong0527/yamaa/blob/main/rust/README.md)
describes crate checks. [Release guidance](release.md) owns migration decisions
and pending acceptance.

## Local checks

Read [AGENTS.md](https://github.com/elong0527/yamaa/blob/main/AGENTS.md) before
editing. Run `make help` from the repository root for supported targets and
prerequisites. The Makefile and CI call the same owned Python command runners.
The recipes below describe component checks; package guides describe installation.
Installed checks require a separate noneditable package outside the checkout.

Use Rust 1.90.0 and the canonical `rust/Cargo.lock`. Python and R source archives
stage the unchanged workspace and lock. From the repository root:

```sh
uv sync --project python --extra test --locked
uv run --project python --no-sync python rust/tools/check_release_api.py
uv run --project python --no-sync python rust/tools/check_shared_release_versions.py
uv run --project python --no-sync python .github/scripts/yaml-validation/test_validate_repository.py
uv run --project python --no-sync python .github/scripts/yaml-validation/validate_repository.py
```

From `rust/`:

```sh
cargo fmt --all --check
cargo clippy --locked -p yamaa-core -p yamaa-engine -p yamaa-adapters -p yamaa-python --all-targets -- -D warnings
cargo test --locked
cargo test --locked -p yamaa-engine --features test-support
python tools/check_dependencies.py
uv run --project ../python --no-sync python -m unittest discover -s tests -p 'test_*.py'
```

The engine feature check includes the integration binaries omitted by a default
test invocation. Portable math has shared authored golden values in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`; run
`cargo test --locked -p yamaa-core --test portable_math_golden` from `rust/`.
Installed package suites run outside the checkout, against built wheel/source
packages. Their commands and comparison rules are in
[qualification](https://github.com/elong0527/yamaa/blob/main/rust/qualification/README.md).
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
Other site pages live in `docs/`. From the repository root:

```sh
python -m pip install -r docs/requirements.txt -r .github/scripts/benchmark-docs/requirements.txt
python -m unittest discover -s .github/scripts/docs -p 'test_*.py'
python .github/scripts/benchmark-docs/generate.py --all --quiet
python -m mkdocs build --strict
```

[CI guidance](https://github.com/elong0527/yamaa/blob/main/.github/ci.md)
defines scoped checks and the required complete native matrix before merging
native, R glue, packaging or native CI changes. [Release guidance](release.md)
separates merged implementation, qualified revisions, pending decisions and
publication. Keep current guidance here and there; retain historical plans in
Git and put new evidence in the relevant PR/issue and qualification records.
