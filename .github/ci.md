# GitHub Actions

Keep workflow definitions in `workflows/` and their supporting code in
`scripts/`, grouped by purpose:

- `scripts/benchmark-docs/`: deterministic HTML dashboards generated from benchmark
  fixtures, their shared template, and checks for source fidelity and freshness.
- `scripts/yaml-validation/`: Python repository and validation blocker checks
  and their unit tests.
- `scripts/ci/`: PR smoke selection, native matrix selection, cohort
  orchestration, and dashboard change detection, with their unit tests.
- `scripts/docs/`: canonical site page hooks and their tests.

## PR feedback

`pr.yml` is the only automatic PR workflow. Three Linux jobs start in parallel:

- `repository-validation (ubuntu-latest)` tests the canonical site page hook,
  Python formatting and lint, CI selection tests, documentation,
  rule metadata and generated references, and shared release versions. It
  installs only PyYAML, Ruff and MkDocs. README and rule pages retain one source;
  no copied landing pages are required.
- `pr-rust-smoke` checks Rust formatting and dependency direction, shared library
  unit tests, parser and numerical tests, feature-enabled project preparation,
  and selected public execution boundaries. It uses the development profile.
- `pr-python-smoke` installs one locked Python 3.12 development-profile wheel
  and tests the retained public helpers, native metadata binding, package/build
  backend, lightweight Harbor harness regressions, and benchmark style.

Runtime jobs inspect the PR merge-base diff and skip build/test steps on
prose-only changes. Renames retain both paths and deleted runtime files still
select coverage. Rust, schema and benchmark data changes select both smoke jobs;
Python, R facade, Harbor and validator changes select Python smoke. Rust test
Markdown fixtures retain coverage. The jobs still report success when their
runtime steps are intentionally skipped, so required checks always appear.

The target is under five minutes for most PRs with warm caches, excluding runner
queue time. The static job has a five-minute timeout. Runtime jobs allow ten
minutes for cold toolchains or dependency changes; a timeout fails the check.
Development builds omit debug information and release LTO. Main pushes run the
same smoke jobs to refresh Rust caches for new PRs. New commits cancel stale PR
runs. Actual Actions timings should be checked after rollout; local timings do
not establish hosted-runner performance.

Configure branch protection to require the three names above, replacing any old
installed-matrix, R, numerical or Harbor PR requirements. The repository check
keeps its previous name. Full qualification has separate check names and is
available on main or by manual dispatch.

## Native coverage

Pushes to main and explicit `qualification/issue-*` branches retain the full
three-OS, three-Python, two-package-form matrix, all three R hosts, shared Rust
checks, the cross-platform math comparison, and the complete public/producer
cohort gate. Manual native dispatches retain the full installed matrix and
numerical obligations. PR smoke checks do not establish release or platform
qualification; source packaging, optimized behavior, the complete integration
suite and installed R behavior remain covered by these full runs. R lanes install
declared suggested test dependencies before strict `R CMD check`; installation
alone does not establish **Status: OK**. Ordinary dispatch omits the shared
Rust/math qualification and audit; retain a full push run for final acceptance.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. The full native evidence audit stays
observational on main and must pass on qualification branches. Complete repository
semantic validation and deterministic dashboard generation run on main or a
manual `yaml-validation.yml` dispatch, along with live validation blocker issue
checks. R grammar/rollback and standalone numerical workflows remain available
outside automatic PR feedback.

For release qualification, platform-specific changes, or changes needing complete
installed evidence before merge, dispatch the native matrix on the branch head:

```sh
gh workflow run native.yml --ref <branch>
gh run list --workflow native.yml --branch <branch> --event workflow_dispatch
gh run view <run-id>
```

The same dispatch is available in GitHub Actions under **Native installation
prototype**, **Run workflow**, selecting the PR branch. Dispatch runs the same
eighteen installed Python lanes, three R hosts, and public/producer cohort gate
as a main push. Require every enabled lane, including Windows R, to pass on that
exact head when using this qualification gate. Shared Rust checks and the math
comparison run on main; retain that combined run for work-package acceptance.
When dispatch access is unavailable, push the reviewed head to an explicit
`qualification/issue-<number>` branch. Its complete installed, shared Rust,
numerical and audit checks supply exact-head premerge evidence. Its audit records
`qualification_scope: reviewed-branch`; renew combined-main evidence after merge.

The shared-crates step also runs
`cargo test --locked -p yamaa-engine --features test-support` on every selected
Rust platform on main and qualification pushes. This enables the feature-gated
project-preparation and test-support coverage omitted by a default-feature test
invocation, including the explicit `test_support` integration suite.
The shared Rust test command also explicitly enables `arrow-c-data` for the
adapters' `c_data_table` integration binary, so Arrow C stream ownership and
boundary controls run on every selected Rust platform.

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
The job also independently replays the original `input` directory in an isolated
network namespace without Actions credentials, requires its complete verdict and
exact JSON agreement, and retains the second record under `replay/` in both audit
artifacts.
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

PR Python smoke runs lightweight image, hardening and fairness regression tests
without installing Harbor or building Docker images. Full R/Python reference
grading runs on relevant main pushes or manual `harbor-evaluation.yml` dispatches.
Oracle verification runs on relevant main pushes, the weekly schedule, or manual
dispatch. It retains all 134 confident opensas cases and the ten-task R/Python
preflight. The summary and `harbor-verification` artifact record the full selection.
