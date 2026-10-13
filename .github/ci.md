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

Native PR runs use Linux for Rust checks, distribution builds, and R package
checks. Installed Python qualification covers wheel and source installs on
Python 3.12 and 3.14. The workflow summary records this reduced matrix.
macOS, Windows, Python 3.13, cross-platform math, and complete public/producer
cohort comparisons run on main pushes or manual full-matrix dispatches.

Pushes to main retain the full three-OS, three-Python, two-package-form matrix,
all three R hosts, the cross-platform math comparison, and the complete
public/producer cohort gate. PR artifacts cover the reduced Linux matrix;
they do not establish full platform qualification. Platform-specific regressions
and Python 3.13 compatibility are checked in the full matrix.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. Native PR runs use eight jobs: the
selection job, Linux Rust checks, a Linux distribution build, four installed
Python qualifications, and Linux R package checks. Full dispatches use 30 jobs;
main pushes add the nonblocking evidence audit for 31 jobs in total.

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
retain the combined main run for work-package acceptance. A reduced PR run does
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

### Independent native run audit

Main pushes append a nonblocking `native-run-evidence-audit` job after all native
lanes finish. Its `native-run-audit` artifact retains `qualification.json` and
`qualification.md`, the exact source revision and parents, selected Actions
attempt metadata, downloaded artifacts, and canonical source files. The audit
job itself may still be running when the metadata snapshot is taken; acceptance
uses the named dependency jobs and never treats the workflow's pending overall
conclusion as a pass.

To audit a completed full dispatch or main run locally, authenticate `gh` for
repository and Actions reads, then use a fresh output directory:

```sh
python rust/tools/audit_native_run.py --run-id 38167427581 \
  --repository elong0527/yamaa --output /tmp/yamaa-native-audit --check
```

To replay downloaded evidence without network access, pass the retained `input`
directory using `--input /tmp/yamaa-native-audit/input` and another fresh
`--output`. This snapshot contains `run.json`, `source.json`, `jobs.json`,
`artifacts.json`, canonical files under `source/`, and artifact directories under
`artifacts/` using their Actions artifact names. An artifact-only directory also produces a
record: missing metadata and exact source remain unavailable, and no tuple can
pass. `--check` returns failure for
failed or missing evidence after writing both records; omit it for a report-only
local audit. API artifact digest fields are discarded, and package checks compare
actual archive and wheel bytes.

The record names all 18 installed Python combinations, three R hosts, nine math
assessments and their exact comparison, 18 complete public cohort and producer
comparisons, 21 corpus-wide public inventories, six package build records, and three shared-crates checks.
Every missing, expired, failed, stale, wrong-attempt or incomplete tuple remains
visible. CI-script discovery runs independent fixture tests for this protocol.
