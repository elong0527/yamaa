# Contributing to yamaa

Start with [AGENTS.md](AGENTS.md), the repository map and hard rules for people
and coding agents. Language behavior belongs in [rules/](rules/README.md) and
[yaml/](yaml/README.md). Independently authored benchmark expectations are the
test oracle; reproduce a discrepancy independently before changing that truth.
Until 1.0.0, a change that breaks a user-facing interface needs an owner
decision and a one-line release-note entry (see AGENTS.md). Remove obsolete
interfaces and their callers together instead of keeping compatibility
aliases. From 1.0.0, the stability contract of #3030 applies.

## Where changes belong

| Area | Responsibility |
| --- | --- |
| `rules/`, `yaml/` | Normative contracts, diagnostics, schemas and grammar fixtures. |
| `rust/crates/yamaa-core/` | Specification admission, expressions, dataset execution, project lifecycle, verification and Define rendering. |
| `rust/crates/yamaa-adapters/` | Codecs, storage formats, physical IO and filesystem environment capture. |
| `rust/crates/yamaa-python/`, `rust/crates/yamaa-r/` | Host arguments, callbacks, scalar conversion and owned result views. |
| `python/`, `R/yamaa/` | Public host packages and package builds. |
| `benchmarks/` | Authored specifications, inputs, metadata and expected results. |
| `evaluations/harbor/` | Agent tasks, reference solutions and grading. |
| `tools/` | Repository checks, documentation, conformance and their tests. |
| `.github/workflows/` | CI orchestration, calling the same commands used locally. |

Keep semantics in shared Rust code. Host facades translate types and retain
owned views; they do not reimplement the language. Build computes and retains
results. Explicit Save publishes held bytes in contract order. Diagnostic codes
belong to the shared registry. Resource policies and supported platforms have
one owner in [release guidance](docs/release.md).

The workspace has four crates. `yamaa-core` owns preparation, project ports,
activation and the complete build lifecycle; its dataset model owns execution.
`yamaa-adapters` implements physical boundaries, and the two binding crates
implement their host boundaries. Core may use `std`; portable math still uses
the pinned libm policy.

The [shared host interface](rust/interface/README.md) owns the binding-generation
design, declaration input, version negotiation and lifetime contract. Public
diagnostic/result records are imported from their shared adapter owner.

Use the Rust toolchain selected by `rust/rust-toolchain.toml` and the unchanged
canonical `rust/Cargo.lock`. The workspace `rust-version` in `rust/Cargo.toml`
is the oldest Rust that builds the engine; the R and Python source packages
build with the caller's toolchain. The workspace keeps Cargo's resolver 2,
which that Rust reads and which ignores each crate's `rust-version`, so update
the lock with Cargo's MSRV-aware fallback:

```sh
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update -p <crate>
```

`make check-repository` fails when a locked package that a supported target
compiles needs a newer Rust than the workspace declares, or when any locked
package uses an edition that Rust does not read
(`tools.packaging.check_minimum_rust`).

One tool, `tools/staging/engine_bundle.py`, stages the engine source bundle
that each host's source package builds from: the engine crates and that
host's binding crate, a lock trimmed from the canonical one and verified crate
by crate against it, and the crates.io crates that the lock selects, vendored,
with no tests or toolchain file (#3030 D55). It builds the Python source
archive, maturin's output over the bundle, with maturin 1.9.6 on the PATH:

```sh
uv run --project python --no-sync --with maturin==1.9.6 \
  python -m tools.staging.engine_bundle sdist dist
```

`python -m tools.staging.engine_bundle bundle {python,r} <new directory>`
writes a host's bundle alone, and `make check-repository` verifies both hosts'
trimmed locks (`tools.packaging.check_cargo_lock`). The R source package still
carries the workspace, lock and toolchain file unchanged until it moves onto
the bundle (#3105), so the toolchain file names only what a build needs; add
Clippy and rustfmt for repository checks with `rustup component add clippy rustfmt`
from `rust/`, as `python -m tools.ci.toolchain --install` does in CI.
The [source packaging guidance](docs/release.md#source-package-contents) links
the vendored licenses and patch provenance.

Every package ships the third-party notices generated from `rust/Cargo.lock`
and from the standard library notice of the toolchain that
`rust/rust-toolchain.toml` pins. After changing the lock or the pinned
toolchain, regenerate them and commit the results. The generator runs `rustc`
in `rust/`, where rustup selects the pinned toolchain:

```sh
uv run --project python --no-sync python -m tools.packaging.generate_third_party_notices --write
```

`make check-repository` fails while a locked crate has no matching notice entry
or the Python License-Expression misses a license that a linked crate is relied
on under. The generator reads only the crate packages Cargo has unpacked. It
stops instead of guessing when a crate needs a license outside its reviewed
list or ships an unclassified license file; review the crate and extend the
tables in `tools/packaging/generate_third_party_notices.py`.

## Develop and verify

Run `make help` at the repository root for prerequisites, configuration variables
and supported targets. Its inventory is the command reference. Set
`CARGO_TARGET_DIR` when separate worktrees need independent Rust build output.
After Rust changes, rerun `make setup`: it rebuilds the Python extension whenever
a file listed under `tool.uv.cache-keys` in `python/pyproject.toml` changes.

Before a push, run `make quick` and the tests nearest the change. It runs lint
and the repository checks, leaving out only the R binding's Clippy, and no test
suite, so it needs no R. Pull request CI runs the complete `make check`; when a
job fails, reproduce it with that job's own target below. Run `make setup` again
after changing `rust/` or `python/src/` before Python tests, which import the
installed package.

| Command | Purpose |
| --- | --- |
| `make setup` | Install the locked development environment. |
| `make quick` | Run lint without the R binding and the repository checks, before a push. |
| `make check` | Run the checks required by ordinary CI. |
| `make check-lint` | Check formatting and lint across Python and Rust. |
| `make check-repository` | Check authored specifications, layout, rules, references and the site. |
| `make test-rust` | Run `cargo test --locked --workspace`, including both host bindings. |
| `make test-python` | Run package/tool tests and standalone installed-host suites. |
| `make test-r` | Build, install and strictly check a fresh R source package. |
| `make check-equivalence` | Require unchanged results from fresh builds of both hosts (executor gate). |
| `make probe-executor` | Time the executor probe on a release build; not part of CI. |
| `make docs-test docs` | Test page ownership, generate dashboards and build the strict site. |

For a focused Rust change, run `cargo test --locked -p <crate>` from `rust/`.
The workspace test command needs Python and R 4.6.1 installed because it builds
and links both binding test targets. `make test-rust` selects its development
Python interpreter for PyO3. On Windows, run
`python -m tools.ci.r_import_library` after installing R and the MSVC build tools
to create the required import library. The Python extension feature belongs to
maturin packaging; enabling every feature would select extension linking instead of
normal Python linking for these Cargo tests. Ordinary CI runs them in the
development profile, with Clippy, on Linux; native qualification runs them on
Linux, macOS and Windows with the optimized `release-test` profile alone.
Cargo discovers integration tests in each crate's `tests/*.rs` files. Add a
feature file there to run new cases with the workspace command; no harness
registration is needed. Keep test bodies in those files and shared private
helpers under that crate's `tests/support/`. Test helpers belong to their owning
crate; do not import another crate's source into tests.
Portable numerical behavior has independently authored controls in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

The Python and R READMEs describe user installation, and the
[R and Python API](docs/articles/api.md) page describes both host APIs.
Installed-host
[qualification](rust/qualification/README.md) owns distribution checks: tests
run outside the checkout against an installed package, with independent inputs
and expected bytes. A local source import does not establish package ownership
or release qualification. Shared host fixture data lives in
`rust/tests/host-fixtures/`; host readers only decode that truth.
Repository orchestration imports the installed development tools package.
Standalone installed suites use copied fixtures and readers; they must not
import repository packages.

For rule or schema changes, follow their area guides; `make quick` runs the
repository checks. `make check-repository` runs `tools.rules.check`, which reads the
rules once through `tools/rules/model.py` and checks cards,
every rule citation and markdown links. Run
`python -m tools.rules.migrate statements --base origin/main` to list
the card titles and statements a pull request adds, changes or removes for
review.

Repository Python checks source syntax, physical schema includes, fixture
metadata, grammar documents, layout and editorial rules without importing an
installed engine. Its complete-corpus test refuses imports of
yamaa, Polars and PyArrow. Shared Rust tests own schema admission, inheritance,
expressions, source binding, execution and the unchanged grammar, regex and
exact rounding fixtures directly.

Installed Python and R conformance compares actual public results and saved
artifacts with independent truth. A negative benchmark's `expected/error.yaml`
is a partial structured assertion: the expected first issue, every declared
path and context facts must match. Complete authored reports additionally pin
all findings, exact values, run reports, callbacks and artifact observations.
Passing static repository validation does not establish engine release
qualification.

A change to the executor, such as rewrite 1 (#2785), must leave every result
unchanged. Each such PR passes ordinary CI and `make check-equivalence` on its
exact head, and says so in the PR. The gate builds the Python and R packages
from the checkout and installs both outside it. On each host it runs public
benchmark conformance: every active benchmark must reproduce its expected
bytes, and every negative benchmark its first diagnostic's phase, condition,
rule and specification paths. A repeat pass runs each entry's spec
object a second time and requires it to reuse what the first run compiled,
with the same issues, tables and saved bytes. On the same installs it then
runs the installed Python suites and `R CMD check`, which carry the
cross-host diagnostic parity truth and the host-fixture families.
`make probe-executor` times one specification at 10,000, 100,000 and
1,000,000 rows, with and without one keyed intermediate lookup;
`examples/executor_probe.rs` in the `yamaa` crate documents its shapes and
arguments. Record its timings on an issue, never in the repository.

## CI and release qualification

The ordinary CI workflow runs the same check targets listed above for each PR,
every two hours on main, and on each push to a `release/**` branch. Merges to
main do not start workflows other than the documentation site deployment; main
is checked on those schedules instead. While the newest main run of CI or the
documentation deployment fails, the `Main health` workflow keeps a `main-red`
issue open for it and closes the issue when a later main run succeeds. A
failing fuzz run on main opens such an issue, or comments on the open one, and
a person closes it: a later green run does not show that the crash is fixed,
because the next run may not find the same input again. A PR that changes
`.github/` also runs actionlint over every workflow.
Keep workflow-specific setup in CI and reusable check logic in `tools/`.
Pin external actions to complete commit IDs with exact release comments.

PRs and the main schedule run four development jobs: repository checks
(including lint), the Rust workspace, and the Python and R hosts on Linux.
These four checks are required to merge into main.

The `Release compatibility` workflow checks the minimum Rust toolchain, the R
source package built with it, minimal format builds, Python 3.15 and minimum
Python saved artifact readers on each `release/**` push and on dispatch. CI's R job checks minimum R saved
artifact readers on those same release events. The `R-CMD-check` workflow
checks macOS, Windows and R-devel packages on release pushes and dispatch.
Dependency graphs and advisories, native qualification, Harbor and coverage
run on dispatch (native qualification also on a
`qualification/issue-*` push). Dispatch any of them on a branch with
`gh workflow run <workflow> --ref <branch>` to check a change that needs them
before merging.

The `Fuzz` workflow fuzzes the readers that parse untrusted input under
AddressSanitizer, with UndefinedBehaviorSanitizer for the C they compile. A PR
runs, for three minutes each, the targets whose readers it changes; a nightly
schedule runs every target for thirty minutes. The repository checks also run
Clippy over the targets, so a change that breaks one fails on its PR. The
[fuzz guide](rust/fuzz/README.md) lists the targets and runs them locally with
`python -m tools.ci.fuzz`. Fix a crash with a unit test in the owning crate and
commit its input as a text regression beside the targets.

The `Release compatibility` workflow's `rust (minimum toolchain)` job checks
and lints the workspace with its declared `rust-version`, Rust 1.82.0, instead
of the pinned toolchain that every other job uses, because embedders and the
source packages build with their own toolchains. Rust 1.82.0 was released on
2024-10-17, so it meets CRAN's request to test with a cargo at least two years
old on any submission date.
Run it locally from `rust/`, with R installed:

```sh
rustup toolchain install 1.82.0 --profile minimal --component clippy
RUSTFLAGS='-D warnings' cargo +1.82.0 check --locked --workspace --all-targets --all-features
RUSTFLAGS='-D warnings' cargo +1.82.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
```

The same job builds the facade from the outside workspace in
`rust/tests/embedder`, as a project that depends on it by path does, and runs
the vendored YAML parser's compact-flow controls directly and through the
facade. Run it from the repository root; the copied lock keeps the run on the
reviewed versions:

```sh
cp rust/Cargo.lock rust/tests/embedder/Cargo.lock
RUSTFLAGS='-D warnings' cargo +1.82.0 test --manifest-path rust/tests/embedder/Cargo.toml --target-dir rust/target
```

It also builds the facade with no optional formats and with YAML + Parquet.
These two builds run only the focused adapter capability tests, verify the
facade's dependency graph and linked native symbols, and check the core's
dependency isolation once. Ordinary workspace tests cover all default formats.
Run both minimal builds from the repository root with the minimum toolchain:

```sh
RUSTUP_TOOLCHAIN=1.82.0 python -m tools.ci.format_features
```

The workflow's `r (minimum toolchain)` job stages the R source package with the
pinned toolchain, which vendors the crates, then builds it and checks it with
`tools/commands/check_r.py` (below) while Cargo and rustc are Rust 1.82.0, and
confirms from the installation log that the build used them. Repeat it from
the repository root, with `R_LIBS` naming a library that holds the package's
Suggests:

```sh
python -m tools.staging.stage_r_package /tmp/yamaa-r-minimum/yamaa
cd /tmp/yamaa-r-minimum && R CMD build --no-manual yamaa
RUSTUP_TOOLCHAIN=1.82.0 python -m tools.commands.check_r --archive yamaa_*.tar.gz --work-dir check
```

`.github/workflows/dependency-graphs.yml` checks both locks against the
advisory policy in `rust/deny.toml` and `osv-scanner.toml`, and resolves
minimum and newest compatible graphs, on dispatch. Run a lane locally
with R and the normal development prerequisites installed:

```sh
uv run --project python --no-sync python -m tools.ci.dependency_graphs \
  --graph minimum --python 3.12 --destination /tmp/yamaa-minimum
```

Use `--graph newest` for the upper lane. The destination must be new and outside
the checkout. The minimum lane pins each workspace registry dependency to its
declared lower bound and each Python runtime/test dependency to its floor.
Vendored crates and exact policy pins remain in force. Transitive Cargo
dependencies use the newest Rust-1.82-compatible selection, rather than claiming
support for every historical transitive release. The newest lane starts without
a Cargo lock and resolves all declared ranges with Cargo's MSRV fallback policy.
Both lanes reject a graph that holds a package, for any target, whose
rust-version or edition needs a newer Rust than 1.82. Python resolves
independently for each interpreter. Each lane regenerates its copy's license
notices from the graph it resolved, so the staging check that every bundled
crate has a notice covers that graph. It then builds its own extension and runs
workspace Rust, Python package/installed-host and strict R source-package
checks. CI retains the resolved Cargo lock, Python requirements and Cargo
metadata for reproduction. Neither lane updates the source checkout's locks or
notices. Release builds use the canonical locks, which source archives carry
unchanged or trimmed and verified against them, and the repository check keeps
the committed notices equal to those generated from the canonical lock; these
compatibility lanes supplement [native qualification](rust/qualification/README.md).

Before every merge into `main` or a `release/X.Y.Z` branch, run a fresh
read-only review check with an authenticated GitHub CLI, supplying the exact
final commit:

```sh
python -m tools.ci.review_gate --repository elong0527/yamaa --pr <number> --head <full-commit>
```

Use that command when GitHub CLI API access is available. If environment policy
blocks it, approved authenticated GitHub connector reads must prove every same
predicate from fresh API evidence: the exact head and base, this PR's authentic
completed status and walkthrough/footer chronology, unchanged base history,
and all resolved threads. Re-read the PR head and status immediately before
merging. Large PRs require the complete bounded proof below through those
reads as well. An unavailable CLI does not waive review requirements.

This requires CodeRabbit's authentic `Review completed` status on that commit
and resolved review threads. The PR must have its own authentic summary and
walkthrough, with a final reviewed footer naming that exact head; completion
must follow the walkthrough's last update. The reviewed range must end at that
head. If the summary omits the range, an authentic formal review on this PR,
submitted for this commit before completion, can supply it. A conflicting or
ambiguous summary range cannot use that fallback. Incremental review ranges
are supported. A status shared with another PR, a skipped status alone, or an
earlier-head review does not pass. Any recorded base retarget or force-push
requires a fresh PR and head, even if the base was later changed back to main.
PRs above 300 changed files require bounded coverage below, including when a
shared head already has a successful status. Triage findings outside inline
threads as well.
The fresh manual check above remains mandatory before every merge, except when
CodeRabbit cannot review the PR; AGENTS.md states what replaces it then.
An atomic change above CodeRabbit's 300-file review limit can use bounded
review-only PRs. Each review PR must have a distinct head commit with the
**complete target head tree**, a synthetic base in this repository, and at
most 300 changed raw leaf paths (a rename can consume two paths). Create the
base as a single child of the target head, restoring only that batch's original
old leaves, including file modes, symlinks and gitlinks. Its single head child
restores the full target tree. The batch deltas must cover the target's
merge-base to head delta exactly once; unrelated changes, gaps and overlaps
fail. Each synthetic head must belong to only its review PR. Keep these PRs
open, preserve their bases, and do not merge them. List their actual numbers
once in the target PR body as JSON, for example:

```html
<!-- yamaa-review-batches: [123, 124] -->
```

The gate permits this path only after the authentic target status explicitly
says the diff exceeded the 300-file limit. Missing or pending target reviews
cannot use it. Every batch still needs its own authentic `Review completed`
status and resolved threads; target threads must also be resolved. The gate
also binds each completion to this PR's exact base/head review range and final
reviewed summary footer. The range can come from the summary or the authentic
formal review for that commit. Completion must follow both the summary's last
update and the formal review's submission when that receipt is used. Missing
evidence, another PR sharing the head, or any recorded
base retarget/force-push history requires a fresh review PR and head. This
prevents a completion for an earlier base from covering a different diff.
The gate checks complete Git trees and paginated PR files through the authenticated API,
then rechecks all PR heads, bases, batch metadata and review statuses. It fails
on truncated trees or incomplete evidence. Moving the target source requires
new matching batch heads and fresh completed reviews. Batch metadata belongs
in the PR body, not a committed evidence ledger.

Native qualification is separate: `.github/workflows/native.yml` builds and
installs the supported Python wheel/source and R source combinations on
dispatch, on a `qualification/issue-*` push and weekly on main. It checks public
benchmark conformance, installed host suites, packaging, optimized Rust behavior
and portable math, and runs CRAN's incoming checks on the R source archive that
a release publishes (its `native-cran` jobs). Passing ordinary CI does not qualify a release. The final
merged revision needs complete native evidence, including the R package
checks below. A release tag publishes the native run that its
`Qualified-Run` trailer names, and the `Release rehearsal` workflow, dispatched
before tagging, runs the same selection against the latest successful main run
without publishing;
see [publication](docs/release.md#publication).

Every R package check, in CI's `r` job (`make test-r`), native
qualification, the `R-CMD-check` workflow and the `r (minimum toolchain)` job,
runs `R CMD check --as-cran` through `tools/commands/check_r.py`, with the
settings of CRAN's own checks that `--as-cran` leaves off. It fails on any NOTE, WARNING or ERROR that
`R/yamaa/cran-comments.md` does not explain: its `EXPECTED_NOTES` lists the
explained notes with the lines each may print, and a test keeps that list
equal to the notes that the file's "R CMD check results" section names. To
accept a new note, explain it there and add it to `EXPECTED_NOTES` in the same
pull request; a warning or an error is never accepted. Remote checks (CRAN's
package index, URLs and the clock) stay off unless the caller sets
`_R_CHECK_CRAN_INCOMING_REMOTE_` or `_R_CHECK_SYSTEM_CLOCK_`. The Linux jobs
have `checkbashisms`, and those of CI, native qualification and R-CMD-check
also `aspell`, so the check covers the configure scripts and the spelling of
`DESCRIPTION` as CRAN's does; elsewhere R warns that it lacks
`checkbashisms`, and that one warning passes.

The R tests use the package's suggested packages, `arrow` and `jsonlite`,
only where they are installed, as "Writing R Extensions" asks: a part that
needs a missing one is skipped with a message. Native qualification's
`native-cran (ubuntu-24.04, R devel, no Suggests)` job installs neither, as
CRAN's noSuggests check does. It sets `_R_CHECK_DEPENDS_ONLY_=true`, so the
tests see only the Depends and Imports, and `_R_CHECK_FORCE_SUGGESTS_=false`,
so R reports the missing packages instead of stopping. Every other check
installs them, and R stops there when one is missing. Repeat the job from the
repository root, with libraries that hold neither package:

```sh
python -m tools.staging.stage_r_package /tmp/yamaa-r-no-suggests/yamaa
cd /tmp/yamaa-r-no-suggests && R CMD build --no-manual yamaa && mkdir library
R_LIBS="$PWD/library" R_LIBS_USER="$PWD/library" _R_CHECK_DEPENDS_ONLY_=true \
  _R_CHECK_FORCE_SUGGESTS_=false python -m tools.commands.check_r \
  --archive yamaa_*.tar.gz --work-dir check
```

Documentation deployment and Harbor evaluations have separate workflows.
Harbor owns its task-build, oracle and changed-input checks, and its own
Python project and lock under `evaluations/harbor/`, apart from the package
lock; see its [guide](evaluations/harbor/README.md). A prose change does not
need a solver run.

Keep current architecture and supported behavior in repository docs. Put release
status, run links, artifact identities and remaining failures in issues and PRs;
do not add evidence ledgers or inventory counts to documentation.

## Documentation

Edit `README.md` for the site home and `rules/README.md` for the rules index.
MkDocs reads those canonical files through its configured hook, resolves their
repository-relative links and preserves their edit targets. Do not create
copies under `docs/`. Other site pages live there; the navigation links directly
to this contributor guide. A site page links a repository file outside `docs/`
by relative path, such as `../rules/reference/csv.md`: the site points it at the
file on main, and a copy read at a release tag names that tag's file.

The rule index, rules summary and schema-field references are generated
from `rules/` and `yaml/` on every build. Edit the owning rule or schema, not a
generated page.
Benchmark dashboards likewise render authored fixtures without executing the
engine. Their [guide](tools/benchmark_docs/README.md) describes the
rendered materials. Generated pages and build output are ignored by Git.

`make setup` installs the locked documentation dependencies. The documentation
targets use that same environment; the strict build checks navigation and
links. Deployment uses the same generated pages and site build.

## Benchmark changes

Follow [benchmarks/AGENTS.md](benchmarks/AGENTS.md) for layout, naming, authoring
and independent expectations. The [benchmark factory](automation/benchmark_factory/README.md)
describes proposals and review; it delegates mechanical checks to the supported
commands instead of maintaining another validator. Expected values must never
come from the candidate engine.

## Reporting findings from private studies

Everything in this repository, including issue and comment edit history, is
public. Use an agent approved by your organization for any private material it
reads. Keep confidential material local; do not send its strings to searches or
other services while investigating.

1. Identify the public rule, requirement, schema or benchmark involved.
2. Reproduce the finding with an existing public fixture or invented data and a
   specification written from scratch. Do not sanitize real records or private
   specifications. Confirm the issue on current main and the affected hosts.
3. Draft the matching form from `.github/ISSUE_TEMPLATE/` outside a pushed
   checkout. Include only public references and output from the synthetic run.
4. Read the complete draft for participant or study identity, unpublished
   material, internal paths/URLs and credentials. Scan it using the reviewer's
   sensitive terms; do not upload screenshots or opaque binary attachments.
5. Show the exact draft to the reviewer for accuracy and organizational approval
   before publishing private-study feedback. Apply the same process to replies.

If a public reproduction is unavailable, discuss only the public contract and
keep the evidence within the organization. If private information was posted,
remove it and its edit history, request attachment removal from GitHub Support,
rotate exposed credentials and follow the organization's incident procedure.
