# yamaa Rust workspace

The shared engine prepares specifications, builds domains and renders submission
documents for the Python and R packages. Language contracts live in
[rules/](../rules/README.md), syntax and schemas in [yaml/](../yaml/README.md),
and executable examples with authored expected results in
[benchmarks/](../benchmarks/README.md).

| Crate | Responsibility |
| --- | --- |
| `yamaa-core` | Values, expressions, schema admission and specification compilation. |
| `yamaa-engine` | Execution lifecycle, resource and function ports, verification and publication. |
| `yamaa-adapters` | YAML, CSV, Parquet, files, project environments and submission rendering. |
| `yamaa-python` | Python arguments, callbacks and owned result views. |
| `yamaa-r` | R arguments, callbacks and owned result views. |

Core and engine dependencies exclude host and storage frameworks. Run
`python tools/check_dependencies.py` from this directory to check those boundaries.
The workspace pins Rust 1.90.0 and uses `Cargo.lock` for every build, including
the unchanged workspace staged into Python and R source archives.

## Development checks

Install the Python development environment from the repository root:

```sh
uv sync --project python --extra test --locked
```

Run these commands from `rust/`:

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked
cargo test --locked --features test-support
uv run --project ../python --no-sync python -m unittest discover -s tests -p 'test_*.py'
```

Repository validation runs from the repository root:

```sh
uv run --project python --no-sync python .github/scripts/yaml-validation/test_validate_repository.py
uv run --project python --no-sync python .github/scripts/yaml-validation/validate_repository.py
uv run --project python --no-sync python rust/tools/check_release_api.py
```

The declared API inventory lives in `qualification/release-api.json`.
[Installed package checks](qualification/README.md) compare actual Python and R
results with authored expected data. The workflow recipes in
[.github/workflows/](../.github/workflows/) define the platform matrix and
packaging checks. Host usage and installation commands live in
[python/README.md](../python/README.md) and [R/yamaa/README.md](../R/yamaa/README.md).

Expected benchmark data and Parquet fixtures are authored truth. Preserve their
bytes when changing implementation or repository structure.
