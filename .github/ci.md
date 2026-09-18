# GitHub Actions

Keep workflow definitions in `workflows/` and their supporting code in
`scripts/`, grouped by purpose:

- `scripts/examples/`: Ruby checks for example column labels and dependency
  order, their shared specification discovery helper, and unit tests.
- `scripts/example-docs/`: deterministic HTML dashboards generated from example
  fixtures, their shared template, and checks for source fidelity and freshness.
- `scripts/yaml-validation/`: Python repository and validation blocker checks,
  their unit tests, and `requirements.txt`.

The `grammar-conformance` workflow is the exception to that grouping: it runs
`R/cdiscbuilder/inst/conformance/grammar_conformance.R`, which belongs to the
R implementation it exercises.

Scripts locate repository data relative to their own files, so they can also
run locally from any working directory. Keep tests and shared helpers beside
the scripts they exercise.

Pin external actions to full commit hashes with exact release comments
(`uses: owner/action@<commit> # vX.Y.Z`). Check the upstream release tag when
updating a pin.

See [Repository Validation](yaml-validation.md) for validation scope and local
commands.
