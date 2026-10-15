# yamaa Rust workspace

The shared engine prepares specifications, builds domains and renders submission
documents for the Python and R packages. Language contracts live in
[rules/](../rules/README.md), syntax and schemas in [yaml/](../yaml/README.md),
and executable examples with authored expected results in
[benchmarks/](../benchmarks/README.md).

The [development guide](../docs/development.md) owns crate boundaries, Rust
1.90.0/locked builds and local verification commands. The
[release guide](../docs/release.md) owns approved decisions, pending migration
gates, rollback and publication state. The declared API inventory lives in
`qualification/release-api.json`. Current rule checks and independent schema-owner
controls are described in the [development guide](../docs/development.md#local-checks).
[Installed package checks](qualification/README.md) compare actual Python and R
results with authored expected data. The workflow recipes in
[.github/workflows/](../.github/workflows/) define the platform matrix and
packaging checks. Host usage and installation commands live in
[python/README.md](../python/README.md) and [R/yamaa/README.md](../R/yamaa/README.md).

Expected benchmark data and Parquet fixtures are authored truth. Preserve their
bytes when changing implementation or repository structure.

Layer composition follows [REQ-0632/0633](../rules/specification/composition.md#req-0632):
a null first declaring an optional field is its schema value, including a
missing-value `no_match` handler. A null overriding an inherited field clears
it. An absent `parents`, `parents: []`, and an empty child preserve an
introducing null's meaning; required fields and invalid null values remain
subject to their ordinary admission rules.
