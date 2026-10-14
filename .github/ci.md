# GitHub Actions

Keep workflow definitions in `workflows/` and their supporting code in
`scripts/`, grouped by purpose:

- `scripts/benchmark-docs/`: deterministic HTML dashboards generated from benchmark
  fixtures, their shared template, and checks for source fidelity and freshness.
- `scripts/yaml-validation/`: Python repository and validation blocker checks,
  their unit tests, and `requirements.txt`.
- `scripts/ci/`: native matrix selection, cohort orchestration, and dashboard
  change detection, with their unit tests.

The separate canonical-site-pages job tests the real MkDocs hook against the
root README and rules index. Those pages have no Markdown copies to synchronize.
The repository-validation job runs formatting/linting and structural/native
validation, and generates deterministic dashboard previews when their sources
change in a PR. The R package workflow replays shared grammar vectors and runs
the complete package suite; grammar changes trigger that workflow as well.

Documentation-only changes use the relevant canonical-page, dashboard and strict
site checks in [scripts/docs/README.md](scripts/docs/README.md). Native workflow
path selection excludes benchmark Markdown and top-level Rust prose. Native
code, R glue, packaging and native selectors require the complete installed
qualification route below. Current gate ownership and publication state live in
[docs/release.md](../docs/release.md).

## Native coverage

Native PR runs use Linux for distribution builds and R package checks.
Installed Python qualification covers wheel and source installs on Python 3.12
and 3.14. The workflow summary records this reduced matrix. macOS, Windows,
Python 3.13, and complete public/producer cohort comparisons run on main pushes,
explicit `qualification/issue-*` branch pushes, or manual full-matrix dispatches.
Shared Rust checks and cross-platform math run on main and qualification branch
pushes. Ordinary manual dispatch omits those shared checks, the nine math
assessments/exact comparison and the run audit; its three-platform numeric-tool
component checks do not substitute for them.

Pushes to main retain the full three-OS, three-Python, two-package-form matrix,
all three R hosts, the cross-platform math comparison, and the complete
public/producer cohort gate. PR artifacts cover the reduced Linux matrix;
they do not establish full platform qualification. Platform-specific regressions
and Python 3.13 compatibility are checked in the full matrix.

R qualification installs the package's declared `Suggests` into the isolated R
library before running the complete suites and strict package check. Missing
suggested test dependencies are failures, not a reason to disable
`_R_CHECK_FORCE_SUGGESTS_` or accept a warning in place of `Status: OK`.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. The PR matrix selects a Linux
distribution build, four installed Python qualifications and Linux R package
checks, plus the selected public/producer cohort comparison. Main and
qualification pushes add three shared Rust jobs, the cross-platform math
comparison and the evidence audit. The audit stays
observational on main and must pass on qualification branches.

Before merging changes to native code, R glue, package builds or native CI,
dispatch the complete matrix on the current PR head:

```sh
gh workflow run native.yml --ref <branch>
gh run list --workflow native.yml --branch <branch> --event workflow_dispatch
gh run view <run-id>
```

The same dispatch is available in GitHub Actions under **Native installation
prototype**, **Run workflow**, selecting the PR branch. Dispatch runs the same
eighteen installed Python lanes, three R hosts, and public/producer cohort gate
as a main push. Require every enabled lane, including Windows R, to pass on that
exact head before merging. Shared Rust checks and the math comparison are
deferred to the main push; retain that combined run for work-package acceptance.
A reduced PR run does not replace the installed dispatch gate. When dispatch
access is unavailable, push the reviewed head to an explicit
`qualification/issue-<number>` branch. Its complete installed, shared Rust,
numerical and audit checks supply exact-head premerge evidence. Its audit records
`qualification_scope: reviewed-branch`; renew combined-main evidence after merge.

The shared-crates step also runs
`cargo test --locked -p yamaa-engine --features test-support` on every selected
Rust platform on main and qualification pushes. This enables both
`project_preparation` and `test_support` integration binaries, which a plain
default-feature test invocation omits.
It also explicitly enables `arrow-c-data` for the adapters' `c_data_table`
integration binary, so the reviewed Arrow C stream ownership and boundary
controls run on every selected Rust platform.

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
lanes finish. Qualification branch pushes run the same audit as a required job.
Its `native-run-audit` artifact retains `qualification.json` and
`qualification.md`, the exact source revision and parents, selected Actions
attempt metadata, downloaded artifacts, and canonical source files. The audit
job itself may still be running when the metadata snapshot is taken; acceptance
uses the named dependency jobs and never treats the workflow's pending overall
conclusion as a pass. The small `native-run-audit-record` artifact separately
retains the verdicts and source/run/job/artifact metadata for review; independent
replay still requires the canonical files and original artifact bytes retained in
the full audit. Both records distinguish combined-main from reviewed-branch scope.
The job also independently replays the original `input` directory without Actions
credentials in a fresh process, requires its complete verdict and exact JSON
agreement, and retains the second record under `replay/` in both audit artifacts.
The audit installs independent report and artifact readers from `python/uv.lock`
without installing the candidate engine. Missing readers stay explicit missing
evidence and cannot produce a complete audit.

To audit a completed main run locally, authenticate `gh` for
repository and Actions reads, then use a fresh output directory:

```sh
python rust/tools/audit_native_run.py --run-id <run-id> \
  --repository elong0527/yamaa --output /tmp/yamaa-native-audit --check
```

Manual dispatches retain installed package and cohort evidence. Auditing one
with the same tool reports the missing shared Rust and math evidence; use the
main run for the complete acceptance check.

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

## Harbor Oracle coverage

PRs select changed Harbor benchmarks from their data, prompts, and reference
solutions using the PR merge-base diff. Renames include both benchmark names;
deleted or non-Harbor benchmarks are excluded from the current selection.
Every buildable selected case is verified by both oracle and nop, including all
R/Python prompt tiers. PRs without affected cases skip the Docker steps and
retain harness regression tests.

Shared harness, image, workflow, selection-code, or dependency changes affect
every benchmark and use full coverage. Scheduled and manual runs always retain
all 134 confident opensas cases and the ten-task R/Python preflight.
The workflow summary and `harbor-verification` artifact record the selection.
