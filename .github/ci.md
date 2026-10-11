# GitHub Actions

Keep workflow definitions in `workflows/` and their supporting code in
`scripts/`, grouped by purpose:

- `scripts/benchmark-docs/`: deterministic HTML dashboards generated from benchmark
  fixtures, their shared template, and checks for source fidelity and freshness.
- `scripts/yaml-validation/`: Python repository and validation blocker checks,
  their unit tests, and `requirements.txt`.
- `scripts/ci/`: native matrix selection, cohort orchestration, and dashboard
  change detection, with their unit tests.

The repository-validation job also checks README/landing-page synchronization,
runs Python formatting and linting once, and generates deterministic dashboard
previews when their sources change in a PR. The R package workflow replays the
shared grammar vectors and runs the complete package suite; grammar changes
trigger that workflow as well.

## Native coverage

Each native PR run samples one of the 18 OS/Python/package-form combinations.
The Actions run ID seeds the selection, so a new run samples again and retries
keep the failed configuration. The workflow summary records the selection.
Rust checks and distribution builds use the selected OS, and qualification
installs the selected wheel or source archive on the selected Python version.
Linux and macOS samples also exercise the R package on the same host. Windows
samples retain the explicit unqualified R route. Public and producer cohort
comparisons require the installed evidence for that selected configuration.

Pushes to main retain the full three-OS, three-Python, two-package-form matrix,
both supported R hosts, the cross-platform math comparison, and the complete
public/producer cohort gate. PR artifacts are labeled as sampled coverage;
they do not establish full platform qualification. Main builds remain the
source of full coverage evidence after merge.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. Native PR runs use five jobs for a
Windows sample or six for a Unix sample; full main runs use 29, including the
selection job.

Scripts locate repository data relative to their own files, so they can also
run locally from any working directory. Keep tests and shared helpers beside
the scripts they exercise.

Pin external actions to full commit hashes with exact release comments
(`uses: owner/action@<commit> # vX.Y.Z`). Check the upstream release tag when
updating a pin.

See [Repository Validation](yaml-validation.md) for validation scope and local
commands.
