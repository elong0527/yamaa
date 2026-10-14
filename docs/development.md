# Development

The Python and R packages execute the same Rust engine. The language contracts
are in [rules](https://github.com/elong0527/yamaa/tree/main/rules), the written
schema and grammar in [yaml](https://github.com/elong0527/yamaa/tree/main/yaml),
and executable examples with independently authored expected results in
[benchmarks](https://github.com/elong0527/yamaa/tree/main/benchmarks).

| Crate | Responsibility |
| --- | --- |
| `yamaa-core` | Values, expressions, schema admission and specification compilation. |
| `yamaa-engine` | Execution lifecycle, resource/function ports, verification and publication. |
| `yamaa-adapters` | YAML, CSV, Parquet, files, project environments and submission rendering. |
| `yamaa-python` | Python arguments, callbacks and owned result views. |
| `yamaa-r` | R arguments, callbacks and owned result views. |

Core and engine are `no_std`; host, filesystem and table frameworks belong in
adapters or bindings. Diagnostics use the shared registry. Resource paths retain
their original declaring file. Builds compute held results; explicit Save
publishes retained bytes, preserves original failures and stops on publication
failure. Host interfaces own conversions and callbacks without a second evaluator.
The [Python guide](https://github.com/elong0527/yamaa/blob/main/python/README.md)
and [R guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md)
describe the public calls, installation and host representations.

## Local checks

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
cargo fmt --check
cargo clippy --all-targets --locked
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
