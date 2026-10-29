# Contributing to yamaa

[AGENTS.md](AGENTS.md) is the contract: the repository map, the hard
rules, the design principles, the pull request flow and what to run before
a push. This guide holds the mechanics those rules point to. Language
behavior belongs in [rules/](rules/README.md) and [yaml/](yaml/README.md);
release, platforms and migrations in [docs/release.md](docs/release.md).
Every tool named here documents itself: read its module docstring or file
header before changing what it checks.

## GitHub Codespaces

[Open a yamaa Codespace](https://codespaces.new/elong0527/yamaa) on a
machine with at least 4 cores, 8 GB of memory and 32 GB of storage. The
[dev container](.devcontainer/devcontainer.json) starts from Rocker's
latest `tidyverse` image and adds Node.js LTS, Rust and uv through its
container features; its [setup script](.devcontainer/setup.sh) then
installs the latest stable Python through uv, the latest opensas Linux
release and OpenCode, runs `make setup` and installs the R package from
the same checkout into `~/R/library`. Setup compiles both
bindings and can take several minutes; wait for **yamaa Codespace is ready**
in the creation log. VS Code and new terminals use `python/.venv`. Then,
from the repository root:

```sh
python -c 'import polars, yamaa; print(yamaa.selftest())'
Rscript -e 'library(tidyverse); library(yamaa); print(yamaa_selftest())'
opensas path/to/program.sas
opencode
```

Configure a model provider inside OpenCode first; keep API keys in your
personal Codespaces secrets, never in the repository. After changing Python
or Rust sources, run `make setup` again; to rebuild both packages, rerun
`bash .devcontainer/setup.sh`. After changing the container configuration,
use **Codespaces: Rebuild Container**; **Full Rebuild Container** also
refreshes the base image to the newest R release.

## Where changes belong

| Area | Responsibility |
| --- | --- |
| `rules/`, `yaml/` | Normative contracts, diagnostics, schemas and grammar fixtures. |
| `rust/crates/yamaa-core/` | Specification admission, expressions, binding, dataset execution, project lifecycle, verification and submission metadata composition. |
| `rust/crates/yamaa-adapters/` | Codecs, storage formats, physical IO, the resolver and publisher ports' implementations and filesystem environment capture. |
| `rust/crates/yamaa-application/` | The use cases that prepare, bind, execute, report and save over core's ports, through the adapters, and the integration suite that runs them on the file route. |
| `rust/crates/yamaa-boundary/` | The interface handshake of both host bindings and the census of live native handles. |
| `rust/crates/yamaa-python/`, `rust/crates/yamaa-r/` | Host arguments, callbacks, scalar conversion and owned result views, over the facade. |
| `rust/crates/yamaa-cli/` | The `yamaa` command line over the facade alone, with the notices and component inventory of its release archives. |
| `python/`, `R/yamaa/` | Public host packages and package builds. |
| `benchmarks/` | Authored specifications, inputs, metadata and expected results. |
| `evaluations/harbor/` | Agent tasks, reference solutions and grading. |
| `tools/` | Repository checks, documentation, conformance and their tests. |
| `.github/workflows/` | CI orchestration, calling the same commands used locally. |

Semantics live in shared Rust; host facades translate types and retain owned
views, and never reimplement the language. Build computes and retains
results; an explicit Save publishes held bytes in contract order. Diagnostic
codes belong to the shared registry. The crate edges (core depends on
nothing in the workspace; adapters on core; the application on both; the
facade composes the three; the bindings depend on the facade and
`yamaa-boundary`, the command line on the facade alone) and the rule that
core holds no file or format code are stated and held by
`tools/validation/check_architecture.py` in `make check-repository`
(#3030 section 2.10, D281 on #3673). The
[shared host interface](rust/interface/README.md) owns `bindings.yaml`,
from which both hosts' registrations and public layers are generated.

**Toolchain and locks.** Build with the toolchain of
`rust/rust-toolchain.toml` and the unchanged `rust/Cargo.lock`. The
workspace `rust-version` is the oldest Rust that builds the engine; the R
and Python source packages build with the caller's toolchain. Update the
lock with Cargo's MSRV-aware fallback, since resolver 2 ignores each crate's
`rust-version`:

```sh
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update -p <crate>
```

`make check-repository` fails when a locked package that a supported target
compiles needs a newer Rust than the workspace declares
(`tools.packaging.check_minimum_rust`).

**Packaging.** `tools/staging/engine_bundle.py` stages the engine source
bundle that each host's source package builds from (#3030 D55), and
`make check-repository` verifies both hosts' trimmed locks against the
canonical one (`tools.packaging.check_cargo_lock`). Every package ships its
host's third-party notices (`LICENSE-THIRD-PARTY.txt`) and component
inventory (`COMPONENTS.tsv`), generated from the locks, the pinned
toolchain and the purposes in `tools/packaging/crate_purposes.toml`; the
generators' docstrings and that file's header say what each lists and what
stops generation. After changing the lock, a build feature, a vendored
component or the pinned toolchain, add a purpose line for each new crate,
remove the lines of crates no host ships, then regenerate and commit:

```sh
uv run --project python --no-sync python -m tools.packaging.generate_third_party_notices --write
uv run --project python --no-sync python -m tools.packaging.generate_component_inventory --write
```

The notices generator needs the toolchain's `rust-src` component, which
`make setup` adds; where rustup cannot, set `YAMAA_RUST_SRC_LIBRARY` to the
`library` directory of the pinned release's unpacked `rust-src` archive.
`make check-repository` fails while a committed copy differs from what the
generators give, a shipped crate has no purpose, or a notice names a GNU
license.

### Design principles in practice

`AGENTS.md`'s design principles say what; this is where each one lands.

**The data form that owns a behaviour.** Before writing Rust, find the
form below. A behaviour the form can express is written there, and its
Rust consumer reads it. A behaviour none can express extends the form
once, with its generator or check, in the same pull request.

| Behaviour | Data form | Generated or checked by |
| --- | --- | --- |
| A file type's fields, types and defaults; an expression function's arguments | `yaml/schema_*.yaml` | `tools.packaging.generate_json_schema`, `tools.packaging.generate_shipped_schema`, `tools.packaging.generate_expression_table` |
| The expression grammar, its closed function and reducer lists, its parse cases | `yaml/grammar/*.yaml` | `tools.packaging.generate_expression_table`, `tools.validation.check_expression_conformance` and the core replay of its cases |
| A condition, its phases and its message | `yaml/conditions.yaml` | `tools.packaging.generate_message_templates`, `tools.validation.check_diagnostics` |
| A CDISC closed list | `yaml/vocabulary/cdisc.yaml` | `tools.packaging.generate_cdisc_vocabulary` and the vocabulary checks |
| What a Parquet source must hold to be read | `rust/crates/yamaa-adapters/profile/parquet.toml` | the Parquet reader, which includes it, and `tools.validation.check_parquet_profile` |
| What an expression evaluates to | `yaml/conformance/expressions.yaml`, `yaml/conformance/regex.yaml` | `tools.validation.check_expression_conformance` |
| What a specification produces or refuses | a benchmark under `benchmarks/` | the corpus walk, public conformance, `support::benchmarks::reproduce` |
| What both hosts return for one case | a `rust/tests/host-fixtures/` family | the readers beside `cases.json`, in both installed suites |
| What one crate's operation reports | the crate's `tests/fixtures/` | its readers under `tests/support/` |
| A host function, its arguments and its result members | `rust/interface/bindings.yaml` | `tools.packaging.generate_bindings` |

**One implementation per judgement.** Where one judgement serves two
situations (a check before data is read and a run with it; a declared type
and a captured one), it is one function over an abstraction with two
sources. Where two copies of one thing must exist (a list in Rust and in a
YAML file; a schema and its shipped bytes), one is the source, the other is
generated from it and checked current in `make check-repository`.

**A fixture before a test function.** A test that writes files or
documents, makes one call and compares rows or bytes is a case in one of
the three homes above: a benchmark for a whole specification with its
expected output, a `host-fixtures` family when both hosts prove it, the
crate's `tests/fixtures/` otherwise. A case the form cannot state yet
extends the form, in the reader every case shares. A test function remains
for what no reader reaches, and its first comment names the seam.

**Keep data as data.** Bytes the engine ships verbatim stay in their own
format and are included at build time. A generated Rust file holds code or
a table, never a transcription of another file's text.

**Size has a ceiling.** 3,000 lines for a source or test file, 250 lines
for a function, seven parameters. Clippy holds the function ceilings
(`rust/clippy.toml`, `[workspace.lints.clippy]` in `rust/Cargo.toml`);
`make check-repository` holds the file ceiling, with an allowlist that
names the open issue splitting each file over it. Measure before a push:

```sh
uv run --project python --no-sync python -m tools.validation.check_source_size
uv run --project python --no-sync python -m tools.validation.source_size
grep -rcE 'allow\(clippy::too_many_(arguments|lines)' rust/crates | grep -v ':0$'
```

A file past the ceiling splits by feature (#2413); a function past it
becomes named steps or takes a struct for what its parameters threaded. An
`allow` names the issue that removes it. A generated file is exempt and
says so in its first five lines, `// Generated by tools/packaging/<name>.py`.

## Develop and verify

`make help` is the command reference, with prerequisites and configuration
variables. Give every worktree its own `CARGO_TARGET_DIR` and share the
compilation through sccache, as `AGENTS.md` says (D232 on #3673). The R
package build ignores both: `make test-r` builds the engine inside the
package twice unless `YAMAA_R_CARGO_TARGET_DIR` names an absolute directory
for both installations to share, as CI's `r` job does. After a Rust change,
rerun `make setup` before Python tests: they import the installed package.

| Command | Purpose |
| --- | --- |
| `make setup` | Install the locked development environment. |
| `make quick` | Run lint and the repository checks (no R, no test suite). |
| `make check` | Run the checks required by ordinary CI. |
| `make check-lint` | Check formatting and lint across Python and Rust, except the R binding. |
| `make check-repository` | Check authored specifications, layout, rules, references and the site. |
| `make test-rust` | Run `cargo test --locked --workspace` without the two host bindings. |
| `make test-python` | Run package/tool tests and standalone installed-host suites. |
| `make test-r` | Lint the R binding, then build, install and strictly check a fresh R source package. |
| `make check-equivalence` | Require unchanged results from fresh builds of both hosts (executor gate). |
| `make probe-executor` | Time the executor probe on a release build; not part of CI. |
| `make docs-test docs` | Test page ownership, generate dashboards and build the strict site. |

For a focused Rust change, run `cargo test --locked -p <crate>` from
`rust/`. The workspace tests leave out the two host bindings, which have no
Rust tests: the installed Python suites and the R package check test them,
so the workspace tests need no R. The R binding's Clippy runs in
`make test-r`. Portable numerical behavior has independently authored
controls in `rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

Installed-host [qualification](rust/qualification/README.md) owns
distribution checks: tests run outside the checkout against an installed
package, with independent inputs and expected bytes. Shared host fixture
data lives in `rust/tests/host-fixtures/`; host readers only decode that
truth. Standalone installed suites use copied fixtures and readers and
never import repository packages.

For rule or schema changes, follow their area guides. `make check-repository`
runs `tools.rules.check` over the cards, their `Fails` lines, rule citations
and markdown links. `python -m tools.rules.migrate statements --base
origin/main` lists the card titles and statements a pull request adds,
changes or removes.

A change to the executor must leave every result unchanged. CI selects the
affected work with `tools/ci/pr_changes.py`: engine, benchmark, schema,
shared build and unrecognized paths qualify both hosts, a host-specific
change its host, and root `README.md`, `CONTRIBUTING.md`, `AGENTS.md` and
prose under `docs/` need only documentation checks. Renames and deletions
count both paths. The equivalence gate builds both packages from the exact
head, installs them outside the checkout and runs public benchmark
conformance, a repeat pass that must reuse what the first run compiled,
the installed Python suites and `R CMD check`. CI's `python` and `r` lanes
are that proof; `HOSTS=python` or `HOSTS=r` runs one half, and only to debug
a failure of that gate (D148 on #3030). `make probe-executor` times one
specification at three row counts; `examples/executor_probe.rs` in the
`yamaa` crate documents it, and its timings go on an issue.

## Writing tests

AGENTS.md states the rules; this section says how each language keeps
them. A test is one authored expectation, the smallest input that reaches
the behaviour through its owner's public surface, and one comparison of the
complete result. No snapshot library and no pasted output enters a test.

### Shape

- The file's first comment names the family. Each test's name is the
  sentence it proves; a name that needs `and` is two tests or one table.
  The comment above a test cites the rule or decision it enforces
  (`FILE-11`, `D188 on #3030`), and a test a rule card cites as evidence
  keeps its name.
- Arrange with the owned factories, act once, then compare the whole
  observable record with an authored literal: a diagnostic is its phase,
  condition, rule, paths and context together, including whether
  `source_span` and `operand_route` are absent; a table is its schema and
  rows; a saved artifact is its bytes.
- An assertion inside a loop carries the case's label.
- A helper that builds returns a value and asserts nothing; a helper that
  asserts prints the whole actual value and points at its caller
  (`#[track_caller]`). Both move to the shared tree once a second file
  needs them.
- The input is the two or three rows that reach the behaviour.

### Rust

Each crate's integration tests are one binary, `tests/main.rs`, which
declares every family file under `tests/` as a module (#3556); a test
there fails while a file is left out. A family is its own `[[test]]` only
when it changes process-wide state, such as the working directory, or when
a job builds it alone. Families share one process, so a temporary name
carries its family's prefix. Run one family with
`cargo test --locked -p <crate> --test main <family>::` and one test with
its whole path and `--exact`.

`tests/support/` is the crate's one helper tree, reached as
`crate::support`. Its factories: `support::document::Tree` builds a
document by hand; `support::roots` lowers project functions and codelists
into AST roots; `support::ports` (core) holds scripted ports that log each
effect and panic on an unexpected request; `support::specification_run`
(application) prepares, compiles and lowers a document;
`support::benchmarks` (application) reads what a live benchmark states
(D120) and `reproduce(name)` runs it as public conformance does;
`support::inheritance::Files` resolves parents beside their declaring
file; `support::study` gives a file-route run its identity, issue rows and
`Study`; `support::Scratch` is a scratch directory removed on drop, never
a hand-built `temp_dir()` path. The core, adapters and application trees
hold copies of some of each other's modules (#2158): change every copy, or
`make check-repository` fails (`tools.validation.check_test_support`).

- Authored controls are files under `tests/fixtures/`, read with
  `include_str!`; assert their row count so a truncated fixture fails.
- A test that writes files or documents, makes one call and compares what
  the call reports is a case of the crate's
  `tests/fixtures/diagnostics/*.json`, in the case form that
  `tests/support/diagnostic/form.rs` documents and both crates read
  (#3691, #3798): the operation, its input, the complete issue rows and,
  for a positive case, the output table, the run report rows and what was
  saved. A case of an existing operation is data alone; a new operation's
  reader is declared under `support::diagnostic::operations`. A case the
  form cannot state yet extends `form`, in both copies.
- A variant of a document comes from an exact-once edit,
  `document.edit(from, to)`, never from `str::replace`, which does nothing
  when its pattern is missing.
- Tests that differ in one value are one test over a table, with the
  label in every assertion:

  ```rust
  for (input, expected, label) in [
      (two_rows(), vec![1, 2], "two rows"),
      (header_only(), vec![], "header only"),
  ] {
      assert_eq!(behaviour(&input), expected, "{label}");
  }
  ```

- An inline `#[cfg(test)]` module tests a private function beside its
  code and repeats nothing an integration test proves.
- No `#[ignore]` and no `thread::sleep`. An environment variable serves
  only a declared skip with its reason, and `Command::new` runs only the
  program under test.

### Python

`tools/tests` is `unittest.TestCase` throughout: a module docstring naming
the family, one class per subject, `setUp` creating a
`tempfile.TemporaryDirectory` registered with `addCleanup`, and
`subTest(case=name)` around each case of a table. Outside
`python/tests/installed/`, `python/tests` is plain pytest with `tmp_path`
and `parametrize`. A fixture repository comes from a builder such as
`write_repository` in `tools/tests/rules_fixture.py`, and the repository
root is `tools.paths.ROOT`. Mock the name the module under test looks up,
inside the narrowest `with`; a new `main` takes `argv` so its test calls it
in-process. Skip with `self.skipTest(reason)` when an optional tool is
missing. A walk of the benchmark corpus or a `cargo` invocation runs once
per module. Prefer `assertEqual` on the whole value and `assertRaisesRegex`
on the message to `assertIn`, `assertTrue(any(...))` or a bare
`assertRaises`.

The installed suites under `python/tests/installed/` are named
`installed_<area>.py`. The runner executes each file directly, so its entry
point must run its checks under `if __name__ == "__main__":`; pytest-only
functions are not executed. They run outside the checkout against an
installed package and import `yamaa` and the staged `host_fixtures` reader,
nothing from `tools/`.

### R

`R/yamaa/tests/*.R` are plain scripts: `R CMD check` runs each in a copy of
the tests directory with the staged inputs beside it, and the first failing
assertion ends the file, so a file holds one family, split by feature. A
script sources the shared readers from `host-fixtures/` (`reader.R`,
`context.R` and `assert.R`), where every shared R helper lives, because
that directory is staged beside every suite and into the R source package.
Each case is a `local({ ... })` block that creates its directory with
`tempfile()` and removes it with `on.exit(unlink(dir, recursive = TRUE),
add = TRUE)`. Name every assertion and compare whole values:

```r
stopifnot("the run keeps both rows" = identical(nrow(table), 2L))
expect_identical(as.list(issues[c("condition", "rule")]),
  list(condition = "range_failed", rule = "CHECK-7"), "the failed range check")
```

A loop over cases wraps each in `local()` and labels it with
`stopifnot_case(label, ...)`. A missing suggested package skips the file
with a message.

### Shared truth across hosts

A case that Python and R both check is authored once, in
`rust/tests/host-fixtures/cases.json` under a family its README lists, with
its consumers named in `rust/qualification/supplemental.json`. Each host's
suite reads the family and asserts the same rows. A host suite adds only
what is host-specific: argument conversion, retained ownership, i64 and NUL
handling, callbacks, interrupts and the host's own failure text. The
command line's `parity.rs` compares its `--format json` issues and saved run
reports with the same cases, so a case whose rows differ by host gives them
under `python`, `r` and `cli`.

## CI and release qualification

Pull request CI (`ci.yml`) reports four required checks, `repository`,
`rust`, `python` and `r`. Every lane checks out the exact head and selects
work from the merge base with `tools/ci/pr_changes.py`. A documentation-only
pull request runs the documentation checks and `make docs-test docs` alone;
the other lanes report a skip. Main is checked on a schedule rather than
after each merge, and the `Main health` workflow keeps a `main-red` issue
open while the newest main run of CI, Docs, Dependency graphs, Next
toolchain or Fuzz fails; a person closes Fuzz's, since a later green run
does not show that a crash is fixed. Keep workflow-specific setup in CI
and reusable check logic in `tools/`; pin external actions to complete
commit IDs with release comments. A pull request that changes `.github/`
also runs actionlint.

The other workflows: `Release compatibility` (the minimum Rust toolchain,
the R source package built with it, minimal format builds, Python 3.15 and
minimum saved-artifact readers) and `R-CMD-check` (macOS, Windows and
R-devel) run on `release/**` pushes and dispatch; `Dependency graphs`
(both locks against the advisory policy in `rust/deny.toml` and
`osv-scanner.toml`, and the minimum and newest compatible graphs) runs
weekly and on dispatch; `Fuzz` runs the targets of the
[fuzz guide](rust/fuzz/README.md) for the readers a pull request changes
and nightly for all; Harbor and coverage run on dispatch. Dispatch one on a
branch with `gh workflow run <workflow> --ref <branch>`. Native
qualification (`native.yml`) runs only on main, weekly and at the candidate
and release cuts (D233 on #3673); `AGENTS.md` says what a pull request does
instead.

**Minimum toolchain.** The workspace declares `rust-version` 1.82.0,
released 2024-10-17, which meets CRAN's request for a cargo at least two
years old. CI's Rust lane lints the workspace with it after the pinned
tests, without the R binding; the release lane adds the binding, builds
the facade from the outside workspace in `rust/tests/embedder` and builds
the facade with no optional formats and with YAML + Parquet. Reproduce from
`rust/`, then from the repository root:

```sh
rustup toolchain install 1.82.0 --profile minimal --component clippy
RUSTFLAGS='-D warnings' cargo +1.82.0 clippy --locked --workspace --exclude yamaa-r --all-targets --all-features -- -D warnings
```

```sh
cp rust/Cargo.lock rust/tests/embedder/Cargo.lock
RUSTFLAGS='-D warnings' cargo +1.82.0 test --manifest-path rust/tests/embedder/Cargo.toml --target-dir rust/target
RUSTUP_TOOLCHAIN=1.82.0 python -m tools.ci.format_features
```

**Dependency graphs.** The minimum lane pins each workspace registry
dependency to its declared lower bound and each Python dependency to its
floor; the newest lane resolves every declared range with Cargo's MSRV
fallback. Each lane regenerates its copy's notices, builds its extension
and runs the Rust, Python and R checks, and neither touches the checkout's
locks. Run one locally into a new directory outside the checkout:

```sh
uv run --project python --no-sync python -m tools.ci.dependency_graphs \
  --graph minimum --python 3.12 --destination /tmp/yamaa-minimum
```

**The merge gate.** Before every merge into `main` or a `release/X.Y.Z`
branch, bring the branch up to date with its base and wait until CI is
green on the exact head; never merge while a required job is queued,
running or cancelled (D176 on #3030). The merge needs one completed review
of that head and every thread resolved: CodeRabbit's, checked by

```sh
python -m tools.ci.review_gate --repository elong0527/yamaa --pr <number> --head <full-commit>
```

Codex's completed review of that head when CodeRabbit has not reviewed it,
or, when neither can (CodeRabbit's reviews are exhausted or it skips the
pull request), the owner's maintenance session's own review of the exact
diff, recorded on the pull request as `## Review of <full sha>` (D175 on
#3030). The gate requires CodeRabbit's authentic `Review completed` status
on that commit, a summary whose final reviewed footer names that head with
no review in progress, and resolved threads; a status shared with another
pull request, a skipped status alone, an earlier-head review or any
recorded base retarget or force-push does not pass. Where the GitHub CLI is
unavailable, authenticated connector reads must prove the same predicates
from fresh API evidence, re-read immediately before merging. A pull request
above CodeRabbit's 300-file limit uses bounded review-only pull requests,
each a distinct head with the complete target tree over a synthetic base
in this repository and at most 300 changed paths, covering the target's
delta exactly once, kept open and never merged, and listed in the target
body as `<!-- yamaa-review-batches: [123, 124] -->`. That path opens only
once the target's status says the diff exceeded the limit; every batch
needs its own completed review and resolved threads, and a moved target
head needs new batches. `tools/ci/review_coverage.py` checks the rest.

**Native qualification.** `native.yml` builds and installs every supported
Python wheel, Python source, R source, R binary and command-line
combination, checks public benchmark conformance, the installed host
suites, packaging, optimized Rust behavior and portable math, and installs
each published artifact with the network disabled; `tools/ci/native.py`
declares the matrix and [publication](docs/release.md#publication) says
which run a release tag names. Passing ordinary CI does not qualify a
release.

**R package checks.** Every R check, in CI's `r` job, native
qualification, `R-CMD-check` and the release lane, runs
`R CMD check --as-cran` through `tools/commands/check_r.py`, except
native qualification's valgrind job: its `--valgrind` runs CRAN's valgrind
check without `--as-cran`, which would turn `--extra-arch` off. The
docstring states the rules: it fails on any NOTE, WARNING or ERROR that
`R/yamaa/cran-comments.md` does not explain, on any sanitizer report, and
under `--valgrind` on any memcheck error. To accept a new note, explain it
there and add it to `EXPECTED_NOTES` in the same pull request; a warning
or an error is never accepted. The R source archive carries a base-R smoke
suite; the full suites stay in the repository, staged outside the checkout
by `make test-r`, and use the suggested packages only where installed.
Native qualification's no-Suggests job installs none, as CRAN's check
does. Reproduce it locally:

```sh
python -m tools.staging.stage_r_package /tmp/yamaa-r-no-suggests/yamaa
cd /tmp/yamaa-r-no-suggests && R CMD build --no-manual yamaa && mkdir library
R_LIBS="$PWD/library" R_LIBS_USER="$PWD/library" R_LIBS_SITE="$PWD/library" _R_CHECK_DEPENDS_ONLY_=true \
  _R_CHECK_FORCE_SUGGESTS_=false python -m tools.commands.check_r \
  --ignore-vignettes --archive yamaa_*.tar.gz --work-dir check
```

Source-package builds render the vignette under `R/yamaa/vignettes/`; when
editing it, run its example against the installed package too.
`tools/staging/vendor_exclusions.toml` is the audited allowlist of upstream
development directories left out of source bundles; a new entry is audited
for build-script and `include!` inputs and proved with an offline build.
Harbor owns its checks and its own lock under `evaluations/harbor/`; see
its [guide](evaluations/harbor/README.md).

## Documentation

Edit `README.md` for the site home and `rules/README.md` for the rules
index; MkDocs reads them through `tools/docs/canonical_pages.py`, so there
is no copy under `docs/`. Other site pages live under `docs/`; a page links
a repository file outside `docs/` by relative path, and the site points it
at the file on main. The rule index, rules summary and schema-field
references are generated from `rules/` and `yaml/` on every build, and the
benchmark dashboards from authored fixtures
([guide](tools/benchmark_docs/README.md)): edit the owning source, never a
generated page.

`make check-repository` validates every fenced `yaml` block of the site
pages, the package READMEs and the issue forms against its file type's JSON
Schema (`tools.validation.check_site_examples`). A block whose root
declares `yamaa:` is a whole file and must validate; a block that shows only
part of a file starts with the line `# fragment`, and a block without
`yamaa:` is not checked. `make setup` installs the documentation
dependencies, and `make docs-test docs` runs the page tests and the strict
site build.

## Benchmark changes

Follow [benchmarks/AGENTS.md](benchmarks/AGENTS.md) for layout, naming,
authoring and independent expectations. The
[benchmark factory](automation/benchmark_factory/README.md) describes
proposals and review. Expected values never come from the candidate engine.

## Reporting findings from private studies

Everything in this repository, including issue and comment edit history, is
public. Use an agent approved by your organization for any private material
it reads, and keep confidential material local.

1. Identify the public rule, requirement, schema or benchmark involved.
2. Reproduce the finding with an existing public fixture or invented data
   and a specification written from scratch; do not sanitize real records.
   Confirm it on current main and the affected hosts.
3. Draft the matching form from `.github/ISSUE_TEMPLATE/` outside a pushed
   checkout, with only public references and the synthetic run's output.
4. Check the whole draft for disclosure, title, body, code blocks, file
   names and links included: read it for participant or study identity,
   unpublished material, internal paths or URLs and credentials, and scan
   it mechanically against the reviewer's sensitive terms (sponsor,
   compound, protocol, study identifiers, internal domains), kept in a
   local file that is never committed or posted. Every hit is removed or
   is a public link. The
   [reviewer feedback recipe](automation/reviewer_feedback.md) is the
   canonical procedure, with the scan commands. No screenshots or opaque
   attachments.
5. Show the exact draft to your reviewer for accuracy and organizational
   approval before posting. Apply the same process to replies.

If a public reproduction is unavailable, discuss only the public contract.
If private information was posted, remove it and its edit history, request
attachment removal from GitHub Support, rotate exposed credentials and
follow your organization's incident procedure.
