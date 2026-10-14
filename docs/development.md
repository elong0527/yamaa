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
editing. Use Rust 1.90.0 and the canonical `rust/Cargo.lock`. Install the locked
Python development environment from the repository root:

```sh
uv sync --project python --extra test --locked
```

Choose the checks for the affected area from the existing command homes:

- [Rust checks](https://github.com/elong0527/yamaa/blob/main/rust/README.md#development-checks),
  including the separate engine `test-support` run.
- [Repository checks](https://github.com/elong0527/yamaa/blob/main/.github/yaml-validation.md#local-commands)
  and [installed checks](https://github.com/elong0527/yamaa/blob/main/rust/qualification/README.md),
  which exercise the installed engine outside the checkout.
- [Site checks](https://github.com/elong0527/yamaa/blob/main/.github/scripts/docs/README.md):
  pinned documentation dependencies, canonical-page tests, dashboard generation
  and a strict MkDocs build. Deployment additionally renders Quarto slides.
- [CI coverage and audit commands](https://github.com/elong0527/yamaa/blob/main/.github/ci.md).
  Local checks and sampled PR jobs do not establish the full installed matrix.

[#2153](https://github.com/elong0527/yamaa/issues/2153) owns consolidation of these
recipes into a root Makefile. Keep the recipes in their owning tools while that
change is reviewed; avoid copying CI shell blocks into another guide.

## Documentation and evidence

Edit `README.md` for the site home and `rules/README.md` for its rules index.
The MkDocs hook reads them directly. Ordinary site pages live under `docs/`;
benchmark dashboards are generated from their fixtures. Do not add home/rules
copies or commit generated dashboards.

Record qualification in the implementing PR/issues and retained audit inputs,
with the actual source, run, attempt and artifact identities. Preserve failures
and partial attempts. Git history retains superseded plans; current gate owners
and live machine-readable records are indexed in [release.md](release.md).
Runtime artifact identity is its reference, contract equality uses canonical
JSON, and resource equality compares retained bytes. Packaging-tool lockfile
checksums remain the sole convention exception; add no repository content digest.
