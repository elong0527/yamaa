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
Every sample also exercises the R package on the same host, including Windows.
Public and producer cohort comparisons require the installed evidence for that selected configuration.

Pushes to main retain the full three-OS, three-Python, two-package-form matrix,
all three R hosts, the cross-platform math comparison, and the complete
public/producer cohort gate. PR artifacts are labeled as sampled coverage;
they do not establish full platform qualification. Main builds remain the
source of full coverage evidence after merge.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. Native PR runs use six jobs on every
platform; full main runs use 30, including the selection job.

Before merging changes to native code, R glue, package builds or native CI,
dispatch the complete matrix on the current PR head:

```sh
gh workflow run native.yml --ref <branch>
gh run list --workflow native.yml --branch <branch> --event workflow_dispatch
gh run view <run-id>
```

The same dispatch is available in GitHub Actions under **Native installation
prototype**, **Run workflow**, selecting the PR branch. Dispatch runs the same
eighteen Python lanes, three R hosts and comparison gates as a main push. Require
every lane, including Windows R, to pass on that exact head before merging, then
retain the combined main run for work-package acceptance. A sampled PR run does
not replace this gate.

The shared-crates step also runs
`cargo test --locked -p yamaa-engine --features test-support` on every selected
Rust platform. This enables both `project_preparation` and `test_support`
integration binaries, which a plain default-feature test invocation omits.

Scripts locate repository data relative to their own files, so they can also
run locally from any working directory. Keep tests and shared helpers beside
the scripts they exercise.

Pin external actions to full commit hashes with exact release comments
(`uses: owner/action@<commit> # vX.Y.Z`). Check the upstream release tag when
updating a pin.

See [Repository Validation](yaml-validation.md) for validation scope and local
commands.
