# Contributing to yamaa

Start with [AGENTS.md](AGENTS.md), the repository map and hard rules for people
and coding agents. Language behavior belongs in [rules/](rules/README.md) and
[yaml/](yaml/README.md). Independently authored benchmark expectations are the
test oracle; reproduce a discrepancy independently before changing that truth.
A change that breaks a user-facing interface needs an owner decision and a
migration note (see AGENTS.md). Remove obsolete interfaces and their callers
together instead of keeping compatibility aliases.

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
canonical `rust/Cargo.lock`. Host source packages carry that workspace and lock.
The [source packaging guidance](docs/release.md#source-package-contents) links
the vendored licenses and patch provenance.

Every package ships the third-party notices generated from `rust/Cargo.lock`.
After changing the lock, regenerate them and commit the results:

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
normal Python linking for these Cargo tests. Native qualification also runs
the workspace tests with the optimized `release-test` profile.
Cargo discovers integration tests in each crate's `tests/*.rs` files. Add a
feature file there to run new cases with the workspace command; no harness
registration is needed. Keep test bodies in those files and shared private
helpers under that crate's `tests/support/`. Test helpers belong to their owning
crate; do not import another crate's source into tests.
Portable numerical behavior has independently authored controls in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

The Python and R READMEs describe user installation. The
[Python package documentation](docs/articles/python.md) and the R README
describe the host APIs. Installed-host
[qualification](rust/qualification/README.md) owns distribution checks: tests
run outside the checkout against an installed package, with independent inputs
and expected bytes. A local source import does not establish package ownership
or release qualification. Shared host fixture data lives in
`rust/tests/host-fixtures/`; host readers only decode that truth.
Repository orchestration imports the installed development tools package.
Standalone installed suites use copied fixtures and readers; they must not
import repository packages.

For rule or schema changes, follow their area guides; `make quick` runs the
repository checks. Repository Python checks source syntax, physical schema includes,
fixture metadata, grammar documents, layout and editorial rules without
importing an installed engine. Its complete-corpus test refuses imports of
yamaa, Polars and PyArrow. Shared Rust tests own schema admission, inheritance,
expressions, source binding, execution and the unchanged grammar, regex and
exact rounding fixtures directly.

Installed Python and R conformance compares actual public results and saved
artifacts with independent truth. A negative benchmark's `expected/error.yaml`
is a partial structured assertion: the expected first issue, every declared
path and context facts must match. Complete authored reports additionally pin
all findings, exact values, logs, callbacks and artifact observations. Passing
static repository validation does not establish engine release qualification.

A change to the executor, such as rewrite 1 (#2785), must leave every result
unchanged. Each such PR passes ordinary CI and `make check-equivalence` on its
exact head, and says so in the PR. The gate builds the Python and R packages
from the checkout and installs both outside it. On each host it runs public
benchmark conformance: every active benchmark must reproduce its expected
bytes, and every negative benchmark its first diagnostic's phase, condition,
requirement and specification paths. On the same installs it then runs the
installed Python suites and `R CMD check`, which carry the cross-host
diagnostic parity truth and the host-fixture families. `make probe-executor`
times one specification at 10,000, 100,000 and 1,000,000 rows, with and
without one keyed intermediate lookup; `examples/executor_probe.rs` in the
`yamaa` crate documents its shapes and arguments. Record its timings on an
issue, never in the repository.

## CI and release qualification

The ordinary CI workflow runs the same check targets listed above for each PR,
every two hours on main, and on each push to a `release/**` branch. Merges to
main do not start workflows other than the documentation site deployment; main
is checked on those schedules instead. While the newest main run of CI or the
documentation deployment fails, the `Main health` workflow keeps a `main-red`
issue open for it and closes the issue when a later main run succeeds. A PR that changes `.github/`
also runs actionlint over every workflow.
Keep workflow-specific setup in CI and reusable check logic in `tools/`.
Pin external actions to complete commit IDs with exact release comments.

PRs and the main schedule run only development checks: lint, repository
checks, the Rust workspace, and the Python and R hosts on Linux. Release lanes
check support claims and run on each `release/**` push and on dispatch: CI's
minimum Rust toolchain, Python 3.15 and minimum saved artifact reader lanes,
the optional format feature selections and the macOS, Windows and R-devel
package checks. Dependency graphs and advisories, native qualification, Harbor
and coverage run on dispatch (native qualification also on a
`qualification/issue-*` push). Dispatch any of them on a branch with
`gh workflow run <workflow> --ref <branch>` to check a change that needs them
before merging.

CI's `rust (minimum toolchain)` release lane checks and lints the workspace with its
declared `rust-version`, Rust 1.90.0, instead of the pinned toolchain that every
other job uses, because embedders build the facade with their own toolchains.
Run it locally from `rust/`, with R installed:

```sh
rustup toolchain install 1.90.0 --profile minimal --component clippy
RUSTFLAGS='-D warnings' cargo +1.90.0 check --locked --workspace --all-targets --all-features
RUSTFLAGS='-D warnings' cargo +1.90.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
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
Reviewed local patches and exact policy pins remain in force. Transitive Cargo
dependencies use the newest Rust-1.90-compatible selection, rather than claiming
support for every historical transitive release. The newest lane starts without
a Cargo lock and resolves all declared ranges with Cargo's MSRV fallback policy;
both lanes reject metadata declaring a higher MSRV. Python resolves independently
for each interpreter. Each lane builds its own extension and runs workspace Rust,
Python package/installed-host and strict R source-package checks. CI retains the
resolved Cargo lock, Python requirements and Cargo metadata for reproduction.
Neither lane updates the source checkout's locks. Release builds and source
archives continue to use the canonical unchanged locks; these compatibility
lanes supplement [native qualification](rust/qualification/README.md).

Before every merge, run a fresh read-only review check with an authenticated
GitHub CLI, supplying the exact final commit:

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
dispatch or on a `qualification/issue-*` push. It checks public
benchmark conformance, installed host suites, packaging, optimized Rust behavior
and portable math. Passing ordinary CI does not qualify a release. The final
merged revision needs complete native evidence, including strict R package
checks with `Status: OK`.

Documentation deployment and Harbor evaluations have separate workflows.
Harbor owns its task-build, oracle and changed-input checks; see its
[guide](evaluations/harbor/README.md). A prose change does not need a solver run.

Keep current architecture and supported behavior in repository docs. Put release
status, run links, artifact identities and remaining failures in issues and PRs;
do not add evidence ledgers or inventory counts to documentation.

## Documentation

Edit `README.md` for the site home and `rules/README.md` for the rules index.
MkDocs reads those canonical files through its configured hook, resolves their
repository-relative links and preserves their edit targets. Do not create
copies under `docs/`. Other site pages live there; the navigation links directly
to this contributor guide. A site page links a repository file outside `docs/`
by relative path, such as `../rules/storage/csv.md`: the site points it at the
file on main, and a copy read at a release tag names that tag's file.

Requirement and schema-field references are generated from `rules/` and `yaml/`
on every build. Edit the owning contract or schema, not a generated page.
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
