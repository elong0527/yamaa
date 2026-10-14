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
Development builds omit debug information and release LTO. Main pushes run the
same smoke jobs to refresh Rust caches for new PRs. New commits cancel stale PR
runs. Actual Actions timings should be checked after rollout; local timings do
not establish hosted-runner performance.

Configure branch protection to require the three names above, replacing any old
installed-matrix, R, numerical or Harbor PR requirements. The repository check
keeps its previous name. Full qualification has separate check names and is
available on main or by manual dispatch.

## README test and coverage badges

On main pushes or manual dispatch, `unit-tests.yml` runs all shared Rust
unit/integration tests (debug and release, including test-support and Arrow
C Data), Python package tests, Rust tool tests,
repository helper tests and installed R package tests on Ubuntu 24.04. The
`All unit tests passed` gate fails if any language job fails, is canceled or is
skipped. The README status badge is pinned to main push runs. Python coverage is
measured on 3.14 and R on 4.6.1; installation compatibility across supported
platforms and interpreters remains in the separate native installation workflow.

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
make audit-native RUN_ID="$YAMAA_RUN_ID" REPOSITORY=elong0527/yamaa OUTPUT=/tmp/yamaa-native-audit
```

Set `YAMAA_RUN_ID` to the completed run's ID first.
`PYTHON` must select an interpreter with the locked independent reader
dependencies from `python/uv.lock`; it need not contain the candidate engine.
Manual dispatches retain installed package and cohort evidence. Auditing one
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

## Retained public performance campaign

Push a reviewed, stable source to `qualification/issue-2142-frontier` to retain
one independent-truth attempt for all 27 workloads on the 21 installed tuples.
This route sets capacity tracing for the candidate children and retains original
refusals; it does not establish timing budgets. The same source first needs its
complete successful native run. Push `qualification/issue-1998` only after the
runtime policy and harness stabilize to collect characterization: ten cold
samples and thirty warm samples after three warm-ups, with same-machine prior
Python 0.3.0 comparators built before any measurements. Workflow dispatch remains
available for an explicit native run and purpose. Reusable workflow calls are
not a campaign entry point: their caller context does not identify this exact
performance workflow run.

Both hosts use nine approved dimension variants per logical tuple: 162 Python
jobs and 27 R jobs. Each job retains its variant's 1k, 10k and 100k cases, complete
sampling distributions, and applicable prior Python comparison on that same
actual runner. The index still names 18 Python and three R logical tuples, each with nine
independently bound original artifact segments. No individual case distribution
is split across runners. The auditor rejects unexpected, overlapping, duplicate
or missing segments/cases; the required counts remain 567 candidate cells and
504 prior lifecycle/root-Build comparator cells.
Before any long producer starts, three OS-specific engine-free R preflights must
pass their original checkpoint/PID controls. A platform refusal blocks collection
and retains its original controls and partial bootstrap bytes. Every actual R
producer also runs its own preflight, using pinned jsonlite 2.0.0 in its actual private
library to require the focused checkpoint byte/failure controls with zero skips.
It also proves that the plain R4.6.1 worker PID equals the RSS broker child,
touches 64MiB of pages, preserves its authored nonzero exit and observes at least
that child allocation. Original process/control receipts stay in the segment
artifact; this preflight is an authored control rather than a memory budget.
Its test count, verdict and original log remain separate from timing samples.

Every source attempt preserves raw failures, partial collections, all 21 logical
slots, and source/baseline/harness histories in a Git bundle. Its native,
producer, index and verified storage jobs must all succeed. Successful original
collection/storage is separate from independent collection qualification and
numeric acceptance. Frontier retains independent truth or refusals and its index
without claiming timing acceptance. Root-Build producer and cross-host Python
comparators require their explicit scope dispositions.

Trusted qualification pushes retain unchanged original Actions ZIP bytes in a
new unpublished run/attempt evidence draft and verify authenticated retrieval.
This storage extends beyond 90-day Actions expiry. It never publishes a release,
creates a Git tag, replaces an asset, or approves a budget.
The serial original-storage job allows the hosted maximum 360 minutes for
download, upload and independent byte retrieval of every ZIP; this is transfer
headroom rather than a proven wall bound or numeric performance threshold.

`performance-replay.yml` runs after the trusted characterization completes. It
keeps the original in-progress `run.json` unchanged and adds a separately fetched
`completion-run.json` and completed manifest. A metadata-only catalog derives
complete reader units from the original manifest: currently 189 independently
bound segments, still 21 logical tuples. Legacy whole-tuple campaigns retain their
original units and remain independently replayable. Each trusted reader job
independently validates every original typed view/report/Save/source/clock/RSS
record for all its cases and applicable roles. Its fragment remains
`accepted:false` with budgets `not_evaluated`.

The strict aggregate binds original reader run/attempt/reviewed checkout, exact
successful jobs and immutable artifact IDs/creation intervals. It rejects
missing, duplicate, unexpected or mismatched cases and independently recomputes
raw distributions and ledgers. Collection qualification requires complete 567
candidate and 504 comparator cells plus original native/source/terminal-success
proof. Original fragment ZIPs, API snapshots and reports are verified and appended
to the same unpublished source draft. Failed original or reader attempts cannot
qualify. A fresh reviewed reader run may repair infrastructure while reusing the
same successful immutable source samples.

The measured source, original qualified reader and current aggregate revisions
remain separate. Later `evidence/issue-1998-replay` pushes select the original
source/run/attempt and original successful reader run/attempt/revision through
versioned `rust/qualification/performance-replay.json`. Reviewed same-source
`performance-budgets.json` is applied to those SAME qualified fragments, without
recollecting or repeating typed capture. The original reader snapshot remains
immutable; a separately fetched `reader-completion-run.json` must prove that
attempt terminal-success. Numeric evidence acceptance still requires independent
verification of the actual finalizer's completed successful run/job/report
artifact before migration closure. The compact current campaign index records
that genuine terminal receipt and durable original identities when available.

## Harbor Oracle coverage

PR Python smoke runs lightweight image, hardening and fairness regression tests
without installing Harbor or building Docker images. Full R/Python reference
grading runs on relevant main pushes or manual `harbor-evaluation.yml` dispatches.
Oracle verification runs on relevant main pushes, the weekly schedule, or manual
dispatch. It retains all 134 confident opensas cases and the ten-task R/Python
preflight. The summary and `harbor-verification` artifact record the full selection.
