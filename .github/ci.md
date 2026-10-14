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
- `scripts/commands/`: owned repository, Rust, documentation and installed-host
  commands shared by the root Makefile and full repository/native CI jobs.

Run `make help` at the repository root for supported targets and prerequisites;
the [development guide](../docs/development.md#local-checks) explains their
contexts. `make check-fast` provides formatting/lint and selection controls;
`make check-repository` runs repository/rule guards. Full repository and native
CI call the shared command scripts directly instead of the root Makefile.
Windows native R jobs require GNU make for their build and check environment.
Automatic PR feedback uses the scoped smoke route below; these feedback checks
do not establish installed qualification.

## PR feedback

`pr.yml` is the only automatic PR workflow. Three Linux jobs start in parallel:

- `repository-validation (ubuntu-latest)` tests the canonical site page hook,
  Python formatting and lint, CI selection tests, documentation,
  rule metadata and generated references, and shared release versions. It
  installs PyYAML, Ruff, MkDocs and Material. README and rule pages retain one
  source; no copied landing pages are required.
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
Development builds omit debug information and release LTO. Main pushes that
change a cache key input (the Cargo lock or manifests, the Rust toolchain file,
the PR requirements or `pr.yml`) and a weekly scheduled run refresh the smoke
caches for new PRs; other main pushes would only restore an unchanged cache. New
commits cancel stale PR runs. Actual Actions timings should be checked after rollout; local timings do
not establish hosted-runner performance.

Configure branch protection to require the three names above, replacing any old
installed-matrix, R, numerical or Harbor PR requirements. The repository check
keeps its previous name. Full qualification has separate check names and runs
nightly on main, by manual dispatch or on a qualification branch push.

## README test and coverage badges

On main pushes or manual dispatch, `unit-tests.yml` runs all shared Rust
unit/integration tests (debug and release, including test-support and Arrow
C Data), Python package tests, Rust tool tests,
repository helper tests and installed R package tests on Ubuntu 24.04. The
`All unit tests passed` gate fails if any language job fails, is canceled or is
skipped. The README status badge is pinned to main push runs. Python coverage is
measured on 3.14 and R on 4.6.1; installation compatibility across supported
platforms and interpreters remains in the separate native installation workflow.
Main pushes that change only prose (`docs/`, rule Markdown, the root README,
`rust/*.md` or `mkdocs.yml`) skip it; YAML Validation and Deploy docs check
those. Prose files that unit tests install or read, such as
`docs/requirements.txt`, `docs/assets/` and the style contract, still run it.

The coverage badge combines executable-line counts from cargo-llvm-cov for the
three shared Rust crates, coverage.py for the installed Python facade and covr for
the staged R package. It requires all three reports and weights their percentages
by reported lines. A successful main test run produces `unit-test-coverage` with
the SVG, raw reports, source revision and coverage page. The docs workflow deploys
that artifact to `/coverage/`. When no report is available, including after an
artifact expires or its download fails, it shows `pending` and documentation
deployment continues. Otherwise it reports the latest successful test run,
linked from the page.
No external coverage account or secret is needed. A failure to deploy the report
does not change the independent unit-test result.

For a local docs build before reports are available, create the pending page:

```sh
python .github/scripts/ci/coverage_report.py --unavailable --output docs/coverage
```

## Native coverage

The nightly main run (05:17 UTC), a manual dispatch on main and pushes to
explicit `qualification/issue-*` branches retain the full three-OS,
three-Python, two-package-form matrix, all three R hosts, shared Rust checks, the
cross-platform math comparison, and the complete public/producer cohort gate.
Main is not qualified on every merge: merges arrive faster than the matrix
finishes, so queued per-push runs were replaced before they started. Dispatch
`native.yml` on main when combined-main evidence is needed sooner. Manual
dispatches on other branches retain the full installed matrix and numerical
obligations. PR smoke checks do not establish release or platform
qualification; source packaging, optimized behavior, the complete integration
suite and installed R behavior remain covered by these full runs. R lanes install
declared suggested test dependencies before strict `R CMD check`; installation
alone does not establish **Status: OK**. A branch dispatch omits the shared
Rust/math qualification and audit; retain a nightly or dispatched main run for
final acceptance.

Benchmark Markdown changes trigger documentation checks without rebuilding
the Python/native runtimes. Specification, input, expected-output, and SDK
changes still trigger runtime coverage. The full native evidence audit stays
observational on main and must pass on qualification branches. Complete repository
semantic validation and live validation blocker issue checks run on every main
push or a manual `yaml-validation.yml` dispatch. Deterministic dashboard
generation runs on a dispatch, or on a main push that changed a dashboard source
since the previous main commit. Dashboard sources include the public conformance
planner, MkDocs configuration and authored dashboard assets used by the checks.
R grammar/rollback and standalone numerical workflows remain available by manual
dispatch; native-rust runs the same numerical checks.

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
as the main run. Require every enabled lane, including Windows R, to pass on that
exact head when using this qualification gate. Shared Rust checks and the math
comparison run on main nightly or by dispatch on main; retain that combined run
for work-package acceptance.
When dispatch access is unavailable, push the reviewed head to an explicit
`qualification/issue-<number>` branch. Its complete installed, shared Rust,
numerical and audit checks supply exact-head premerge evidence. Its audit records
`qualification_scope: reviewed-branch`; renew combined-main evidence after merge.

The shared-crates step and `make test-rust` both call
`scripts/commands/rust_tests.py` for locked debug, engine `test-support` and
optimized `release-test` tests. The engine feature enables the feature-gated
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

Nightly and dispatched main runs append a nonblocking `native-run-evidence-audit`
job after all native lanes finish. Qualification branch pushes run the same audit
as a required job.
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
make audit-native RUN_ID="$YAMAA_RUN_ID" REPOSITORY=elong0527/yamaa OUTPUT=/tmp/yamaa-native-audit
```

Set `YAMAA_RUN_ID` to the completed run's ID first.
`PYTHON` must select an interpreter with the locked independent reader
dependencies from `python/uv.lock`; it need not contain the candidate engine.
Branch dispatches retain installed package and cohort evidence. Auditing one
with the same tool reports the missing shared Rust and math evidence; use the
main run for the complete acceptance check.

To replay downloaded evidence without network access, use the retained `input`
directory and another fresh output directory:

```sh
make audit-replay AUDIT_INPUT=/tmp/yamaa-native-audit/input OUTPUT=/tmp/yamaa-native-replay
```

Both Make targets call `rust/tools/audit_native_run.py` with `--check`.
This snapshot contains `run.json`, `source.json`, `jobs.json`,
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
