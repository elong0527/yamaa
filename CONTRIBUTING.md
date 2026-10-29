# Contributing to yamaa

Start with [AGENTS.md](AGENTS.md), the repository map and hard rules for people
and coding agents. Language behavior belongs in [rules/](rules/README.md) and
[yaml/](yaml/README.md). Independently authored benchmark expectations are the
test oracle; reproduce a discrepancy independently before changing that truth.
Until 1.0.0, a change that breaks a user-facing interface needs an owner
decision and a one-line release-note entry (see AGENTS.md). Remove obsolete
interfaces and their callers together instead of keeping compatibility
aliases. From 1.0.0, the [stability contract](docs/release.md#stability-contract)
applies.

## GitHub Codespaces

[Open a yamaa Codespace](https://codespaces.new/elong0527/yamaa) to work in
VS Code in your browser. Select the branch you want and a machine with at
least 4 cores, 8 GB of memory and 32 GB of storage, then create the Codespace.
GitHub clones the whole repository into the workspace, including the
benchmarks, schemas, rules and both host packages.

The [dev container](.devcontainer/devcontainer.json) uses Rocker's latest
`tidyverse` image for the latest released R and a pre-installed tidyverse.
Its [setup script](.devcontainer/setup.sh) installs the latest stable Python
through uv, the latest opensas Linux release, and the latest OpenCode v2 through
its `@opencode/cli@2` npm package. Setup removes the old `opencode-ai` package
when rebuilding an existing Codespace. Node.js LTS and Rust are included for those
tools and for building yamaa.

Setup runs `make setup` to install the Python yamaa package, Polars and the
locked development dependencies into `python/.venv`. It stages and installs
the R yamaa package from the same checkout into `~/R/library`, using the
repository's Rust toolchain. Both packages therefore match the selected
branch. Initial setup compiles both bindings and can take several minutes;
wait for **yamaa Codespace is ready** in the creation log. Setup checks both
packages with their bundled self-tests and runs an opensas smoke program.

The VS Code Python interpreter and new terminals use `python/.venv`. Start
with these commands from the repository root:

```sh
python -c 'import polars, yamaa; print(yamaa.selftest())'
Rscript -e 'library(tidyverse); library(yamaa); print(yamaa_selftest())'
opensas path/to/program.sas
opencode
```

Configure a model provider inside OpenCode before using it; provider
credentials are not included in the image. Store any API keys in your
personal Codespaces secrets rather than in the repository.

After changing Python or Rust sources, run `make setup` again to rebuild the
Python package. To rebuild both packages, rerun `bash .devcontainer/setup.sh`;
this recreates the Python environment. After changing the container
configuration, use **Codespaces: Rebuild Container** in the command palette.
To refresh the base image to the newest R release, use **Codespaces: Full
Rebuild Container**. The lock file continues to select yamaa's development
dependencies; the R image, Python and opensas follow their latest stable
releases when freshly installed, and OpenCode follows the latest v2 release.

## Where changes belong

| Area | Responsibility |
| --- | --- |
| `rules/`, `yaml/` | Normative contracts, diagnostics, schemas and grammar fixtures. |
| `rust/crates/yamaa-core/` | Specification admission, expressions, binding (one walk, which `run()` takes over the captured input schemas and `check()` over the declared ones), dataset execution, project lifecycle, verification and submission metadata composition. |
| `rust/crates/yamaa-adapters/` | Codecs, storage formats, physical IO, the resolver and publisher ports' implementations and filesystem environment capture. |
| `rust/crates/yamaa-application/` | The application layer: the use cases that prepare, bind, execute, report and save over core's ports, through the adapters, and the integration suite that runs them on the file route. |
| `rust/crates/yamaa-boundary/` | The interface handshake of both host bindings, the gate every native call passes and the census of live native handles. |
| `rust/crates/yamaa-python/`, `rust/crates/yamaa-r/` | Host arguments, callbacks, scalar conversion and owned result views, over the facade. |
| `rust/crates/yamaa-cli/` | The `yamaa` command line: its arguments, exit statuses and printed output, over the facade alone, with the third-party notices and component inventory of its release archives. |
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

The workspace has eight crates. `yamaa-core` owns preparation, project ports,
activation and the complete build lifecycle; its dataset model owns execution.
`yamaa-adapters` implements physical boundaries. `yamaa-application`, the
application layer, orchestrates prepare, bind, execute and publish over core's
ports through the adapters and depends on those two alone (#3761), and the
`yamaa` facade composes the three. The two binding crates and the `yamaa-cli`
command line implement their host boundaries and depend on the facade; the
bindings also depend on `yamaa-boundary`, which holds their interface handshake and depends
on nothing else in the workspace (D281 on #3673 restores these edges of
#3030 section 2.10, which D225 had amended). Core may use
`std` but holds no file or format code: it keeps the formats' metadata models,
and the adapters read and write their bytes. Portable math still uses the
pinned libm policy. `make check-repository` holds both rules of #3030 section
2.10 (`tools.validation.check_architecture`): every dependency table of every
crate keeps those edges, and a scan of core's sources, outside test-only items,
refuses the standard library's file, IO, network, process, environment, OS and
path modules and its clock, decoding bytes as text, and the XML, SAS transport
and Parquet marks of a format, with no exception.

The [shared host interface](rust/interface/README.md) owns the binding-generation
design, declaration input, version negotiation and lifetime contract. The
diagnostic record is imported from core, and the boundary version from
`yamaa-boundary`.

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
that each host's source package builds from: the engine crates, the boundary
crate and that host's binding crate, a lock trimmed from the canonical one and
verified crate
by crate against it, and the crates.io crates that the lock selects, vendored,
with no tests and no toolchain file anywhere (#3030 D55). It builds the Python
source archive, maturin's output over the bundle, with maturin 1.9.6 on the
PATH:

```sh
uv run --project python --no-sync --with maturin==1.9.6 \
  python -m tools.staging.engine_bundle sdist dist
```

`python -m tools.staging.engine_bundle bundle {python,r} <new directory>`
writes a host's bundle alone, and `make check-repository` verifies both hosts'
trimmed locks (`tools.packaging.check_cargo_lock`). The R source package
stages the R bundle under `src/rust`, with its crates compressed
(`python -m tools.staging.stage_r_package <new directory>`, which
`make test-r` runs). Given built archives, `tools.packaging.check_cargo_lock`
checks their bundles against the checkout and refuses a toolchain file
anywhere in them. The toolchain file names only what a build needs; for
repository checks, `make setup` adds Clippy, rustfmt and the standard library's
sources where rustup runs, as `rustup component add clippy rustfmt rust-src`
from `rust/` does. In CI, `python -m tools.ci.toolchain --install` adds Clippy
and rustfmt, and its `--notices` option adds `rust-src` only for the jobs that
run the third-party notices generator.
The [source packaging guidance](docs/release.md#source-package-contents) links
the vendored licenses and patch provenance.

Every package ships the third-party notices of its host's crate,
`python/LICENSE-THIRD-PARTY.txt` for the Python packages,
`R/yamaa/inst/LICENSE-THIRD-PARTY.txt` for the R package and
`rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt` for the command-line archives,
generated from `rust/Cargo.lock` and from the standard library notice and the
`rust-src` component of the toolchain that `rust/rust-toolchain.toml` pins.
They list only the crates that the host links on the supported targets, with
the features that its release build enables, as `cargo tree` resolves them
(`BUILD_FEATURES` in the generator, #3470), each with the one license that
yamaa relies on, and no notice names a GNU license (#3364). The standard
library's own crates come from `rust-src`: those that
`library/std/Cargo.toml` declares for the supported targets, at the versions
that `library/Cargo.lock` locks, with the license files of their packages in
`library/vendor` (#3369). The archives for the musl targets also carry
`rust/crates/yamaa-cli/LICENSE-MUSL.txt`, the notice of the musl C library that
their executables link. After changing the lock, the pinned toolchain or a
host's build features (`BUILD_FEATURES`, which a tools test holds to the
builds), regenerate the notices and commit the results. The generator runs
`rustc` and `cargo tree` in `rust/`, where rustup selects the pinned toolchain:

```sh
uv run --project python --no-sync python -m tools.packaging.generate_third_party_notices --write
```

Where rustup cannot add `rust-src`, set `YAMAA_RUST_SRC_LIBRARY` to the
`lib/rustlib/src/rust/library` directory of the pinned release's unpacked
`rust-src` archive from static.rust-lang.org.

`make check-repository` fails while a linked crate has no matching notice entry
or the Python License-Expression misses a license that a linked crate, or a
crate that the standard library links, is relied on under. The generator reads
only the crate packages Cargo has unpacked and the toolchain's own files. It
stops instead of guessing when a crate needs a license outside its reviewed
list, ships an unclassified license file, or would bring in a text that names
a GNU license, and when the standard library links an in-tree crate that
declares a license of its own that `LIBRARY_IN_TREE` does not list (#3494);
review the crate and extend the tables in
`tools/packaging/generate_third_party_notices.py`. A toolchain update
also needs `MUSL` there reviewed, and the check stops until it is: the musl
release that the toolchain bundles for its `*-linux-musl` targets, as
`src/ci/docker/scripts/musl.sh` of the Rust repository names it at the
release's tag, with `LICENSE-MUSL.txt` holding that release's `COPYRIGHT` file.

Every crate of each host's trimmed lock has a purpose in
`tools/packaging/crate_purposes.toml`: the source archives ship them all,
including the crates that only build scripts and procedural macros use and
those that no supported target compiles, the latter as their manifests
alone, and the command line's lock is the graph that its binaries build
from. `make check-repository` fails while a
shipped crate has no purpose, a purpose names a crate that no host ships, or
a purpose is empty (`tools.packaging.check_crate_purposes`). After a
dependency change, add a line for each new crate saying why yamaa ships it, as
the file's header describes, and remove the lines of crates that no host ships
any more. From `rust/`,
`cargo tree --locked --offline --workspace --target all -i <crate>` shows what
pulls a crate in.

The same file's `[unconditional.<engine crate>]` tables say why an engine crate
(`yamaa-core`, `yamaa-adapters`, `yamaa-application` or `yamaa`) depends,
without a feature, on a crate that #3030 section 2.9 scopes to the features and
adapters that need it: an Arrow crate, `flatbuffers`, `serde_json` or `toml`. The same check reads the
engine crates' manifests and fails while such a dependency has no reason, or a
reason names a dependency that is optional or gone, so the change that moves a
crate behind a feature also removes its reason.

Every package also ships `COMPONENTS.tsv` beside the notices: an inventory,
without digests, of the components that its host's packages are built from.
`tools.packaging.generate_component_inventory` writes `python/COMPONENTS.tsv`,
`R/yamaa/inst/COMPONENTS.tsv` and `rust/crates/yamaa-cli/COMPONENTS.tsv` from
each host's trimmed lock, the license that yamaa relies on of those that each
crate declares, those purposes and the provenance that the vendored YAML
parser, ReadStat and zstd record. The Python one, which the wheels carry, and
the command line's also name the standard library of the pinned toolchain;
the Python source archive carries the Python one without that row. The R one
names none: the R package's
`configure` appends the row of the `rustc` that builds the installation to
the copy that R installs. `make check-repository` fails while one differs
from what they give, and while a host's `LICENSE-THIRD-PARTY.txt` lists other
crates than its inventory marks as linked: a crate whose purpose starts with
`build script:`, `procedural macro:` or `not compiled:` is not linked. After
changing the lock, a purpose, a vendored component or the pinned toolchain,
regenerate them and commit the results:

```sh
uv run --project python --no-sync python -m tools.packaging.generate_component_inventory --write
```

### Design principles in practice

`AGENTS.md`'s design principles say what; this is where each one lands in
the repository.

**The data form that owns a behaviour.** Before writing Rust, find the
form below. A behaviour the form can express is written there, and its
Rust consumer reads it.

| Behaviour | Data form | Generated or checked by |
| --- | --- | --- |
| A file type's fields, types and defaults; an expression function's arguments | `yaml/schema_*.yaml` | `tools.packaging.generate_json_schema`, `tools.packaging.generate_shipped_schema`, `tools.packaging.generate_expression_table` |
| The expression grammar, its closed function and reducer lists, its parse cases | `yaml/grammar/*.yaml` | `tools.packaging.generate_expression_table`, `tools.validation.check_expression_conformance` and the core replay of its cases |
| A condition, its phases and its message | `yaml/conditions.yaml` | `tools.packaging.generate_message_templates`, `tools.validation.check_diagnostics` |
| A CDISC closed list | `yaml/vocabulary/cdisc.yaml` | `tools.packaging.generate_cdisc_vocabulary` and the vocabulary checks |
| What a Parquet source must hold to be read: its column types, physical types and encodings, pages and codecs | `rust/crates/yamaa-adapters/profile/parquet.toml` | the Parquet reader, which includes it, and `tools.validation.check_parquet_profile`, which keeps PARQUET-7's table on it |
| What an expression evaluates to | `yaml/conformance/expressions.yaml`, `yaml/conformance/regex.yaml` | `tools.validation.check_expression_conformance` |
| What a specification produces or refuses | a benchmark under `benchmarks/` | the corpus walk, public conformance, `support::benchmarks::reproduce` |
| What both hosts return for one case | a `rust/tests/host-fixtures/` family | the readers beside `cases.json`, in both installed suites |
| What one crate's operation reports | the crate's `tests/fixtures/` | its readers under `tests/support/` |
| A host function, its arguments and its result members | `rust/interface/bindings.yaml` | `tools.packaging.generate_bindings` |

A behaviour none of these can express extends the form once, with its
generator or check, in the same pull request; it is not written by hand
in the form's consumers. Rust holds semantics and execution: what a
function computes, how a table is built, how bytes are read and written.

**One implementation per judgement.** Two code paths that must agree are
a divergence waiting to happen, and the tests that prove their agreement
are its cost. Where one judgement serves two situations (a check before
data is read and a run with it; a declared type and a captured one), it
is one function over an abstraction with two sources, and the findings
and their order are defined once. Where two copies of one thing must
exist (a list in Rust and in a YAML file; a schema and its shipped bytes),
one is the source and the other is generated from it and checked current
in `make check-repository`, as the vocabulary (#3701), the message
templates and the expression table (#3800) are.

**A fixture before a test function.** A test that writes files or
documents, makes one call and compares rows or bytes is a case in one of
the three homes above: a benchmark when the case is a whole specification
with its expected output, a `host-fixtures` family when both hosts prove
it, the crate's `tests/fixtures/` otherwise. The case form carries the
complete expected record (`assert_issues`'s shape for issues; the rows,
or the authored file, for output). A case the form cannot state yet
extends the form, in the reader every case shares, rather than becoming
a test function. A test function remains for what no reader reaches: a
private function's seam, a port's scripted interaction, a panic's
containment, a program the test spawns because it is the subject. Its
first comment names the seam, and the reviewer reports a test function
that a fixture home could have held.

**Keep data as data.** Bytes the engine ships verbatim stay in their own
format and are included at build time, so a review sees a YAML change as
a YAML diff. A generated Rust file holds code or a table, never a
transcription of another file's text.

**Size has a ceiling.** The ceilings are 3,000 lines for a source or test
file, 250 lines for a function, and seven parameters, Clippy's default.
Measure before a push:

```sh
uv run --project python --no-sync python -m tools.validation.check_source_size
uv run --project python --no-sync python -m tools.validation.source_size
grep -rcE 'allow\(clippy::too_many_(arguments|lines)' rust/crates | grep -v ':0$'
```

`make check-repository` holds the file ceiling with the first command: a
hand-written `.rs` file under `rust/crates/` fails over 3,000 lines unless
the check's allowlist names the open issue that splits it. The entry fails
once its file is back under the ceiling, and once `--closed` states that
its issue is closed. The check reads no network, so `make check-repository`
passes no `--closed`; the orchestrator's board run passes the allowlisted
issues that GitHub reports closed. The second command prints each crate's
source, test and generated lines and its functions over 250 lines, as
Clippy counts them. The third lists the `allow`s of the function ceilings.

Clippy holds the function ceilings. `rust/clippy.toml` sets
`too-many-lines-threshold` to 250, and `[workspace.lints.clippy]` in
`rust/Cargo.toml` turns `too_many_lines` on beside `too_many_arguments`,
whose default is seven, so the `-D warnings` Clippy of every push fails a
function over either.

A file past the ceiling splits by feature, as `tests/<area>.rs` files
split by family (#2413). A function past it becomes named steps, each a
function, or takes a struct carrying what its parameters threaded; a
function that is one state machine says so in a comment. An `allow` for
`too_many_arguments` or `too_many_lines` names the issue that removes it.
A generated file is exempt from the file ceiling and says so in one of its
first five lines, `// Generated by tools/packaging/<name>.py`, naming the
script that writes it; the check fails a marker that names no such script.
The per-crate line counts of a change that moves code or tests go on its
issue, never in a committed file.

## Develop and verify

Run `make help` at the repository root for prerequisites, configuration variables
and supported targets. Its inventory is the command reference. Give every
worktree its own `CARGO_TARGET_DIR` and share the compilation through sccache
(`RUSTC_WRAPPER=sccache`, one `SCCACHE_DIR` per machine), as `AGENTS.md` says:
a target shared between worktrees mixes their artifacts, because Cargo names a
workspace crate's outputs without the worktree's path and trusts modification
times (D232 on #3673).
The R package's build ignores it: `make test-r` installs the package twice,
each time building the engine inside the package, unless
`YAMAA_R_CARGO_TARGET_DIR` names an absolute directory for both installations
to share, as CI's `r` job does. Unset, the build also ignores a rustc wrapper,
such as sccache, and a Cargo build directory that the environment or a parent
`.cargo/config.toml` sets.
After Rust changes, rerun `make setup`: it rebuilds the Python extension whenever
a file listed under `tool.uv.cache-keys` in `python/pyproject.toml` changes.

Before a push, run the tests nearest the change and, after a Rust change,
`cargo fmt` and Clippy on the touched crates. Run `make quick` only for a
change to generated files, rule cards, schemas, docs generators or Python
(D148 on #3030). It runs lint and the repository checks, which leave the R
binding's Clippy to `make test-r`, and no test suite, so it needs no R. Pull
request CI runs the complete `make check`; when a job fails, reproduce it with
that job's own target below. Run `make setup` again after changing `rust/` or
`python/src/` before Python tests, which import the installed package.

| Command | Purpose |
| --- | --- |
| `make setup` | Install the locked development environment. |
| `make quick` | Run lint and the repository checks. |
| `make check` | Run the checks required by ordinary CI. |
| `make check-lint` | Check formatting and lint across Python and Rust, except the R binding. |
| `make check-repository` | Check authored specifications, layout, rules, references and the site. |
| `make test-rust` | Run `cargo test --locked --workspace` without the two host bindings. |
| `make test-python` | Run package/tool tests and standalone installed-host suites. |
| `make test-r` | Lint the R binding, then build, install and strictly check a fresh R source package. |
| `make check-equivalence` | Require unchanged results from fresh builds of both hosts (executor gate). |
| `make probe-executor` | Time the executor probe on a release build; not part of CI. |
| `make docs-test docs` | Test page ownership, generate dashboards and build the strict site. |

For a focused Rust change, run `cargo test --locked -p <crate>` from `rust/`.
The workspace test command leaves out the two host bindings, which have no Rust
tests (`test = false` in their manifests): the installed Python suites and the
R package check test them. The workspace tests therefore build neither PyO3 nor
extendr, and need no R and, on Windows, no R import library. `make check-lint`
runs Clippy over the Python binding as well; building the R binding runs R, so
its Clippy runs in `make test-r`. Ordinary CI runs the workspace tests in the
development profile, with Clippy, on Linux; native qualification runs them on
Linux, macOS and Windows with the optimized `release-test` profile alone.
Each crate's integration tests are one binary, `tests/main.rs`, which
declares every family file under `tests/` as a module (#3556): the support
tree compiles once and the families link into one executable. Add a family as
a `tests/<family>.rs` file and its `mod` line in `tests/main.rs`; a test there
fails while a file is left out. A family is instead its own `[[test]]` in the
crate's manifest when it changes process-wide state, such as the working
directory that yamaa's `plan_identity.rs` and `save_table_relative.rs` change,
since every other family shares its process, or when a job builds it alone, as
the minimal format builds build the adapters' `format_features.rs`. Keep test bodies in those files and
shared private helpers under that crate's `tests/support/`. Test helpers belong
to their owning crate; do not import another crate's source into tests.
Portable numerical behavior has independently authored controls in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`.

The Python and R READMEs describe user installation, and the
[R, Python and command-line API](docs/articles/api.md) page describes both
host APIs and the command line.
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
rules once through `tools/rules/model.py` and checks cards, that a card's
`Fails` line names every registered condition, every rule citation and
markdown links. Run
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
unchanged. Each such PR passes ordinary CI with equivalence in both the
`python` and `r` lanes on its exact head. Those checks are the proof: run
`make check-equivalence` locally only to debug a CI failure of that gate
(D148 on #3030). CI selects the affected work with `tools/ci/pr_changes.py`.
Engine, benchmark, schema, shared build and unrecognized paths qualify both
hosts. Host-specific changes qualify that host; known repository-test,
validation and documentation tools run their ordinary tests. Root
`README.md`, `CONTRIBUTING.md`, `AGENTS.md` and prose/site assets under
`docs/` need only documentation checks. Deleted paths count, and renames
count both the old and new paths, so moving code into documentation cannot
skip qualification. The gate builds the Python and R packages from the exact
pull-request head and installs both outside the checkout. On each host it
runs public benchmark
conformance: every active benchmark must reproduce its expected
bytes, and every negative benchmark its first diagnostic's phase, condition,
rule and specification paths. A repeat pass runs each entry's spec
object a second time and requires it to reuse what the first run compiled,
with the same issues, tables and saved bytes. On the same installs it then
runs the installed Python suites and `R CMD check`, which carry the
cross-host diagnostic parity truth and the host-fixture families.
`HOSTS=python` runs the Python half alone: its build, its conformance and the
installed suites; `HOSTS=r` runs the R half: its build, its conformance and
`R CMD check`. CI runs the two halves in the parallel `python` and `r` lanes,
sharing each fresh installation with that host's ordinary tests. The installed
suites and R package check run once. Both lanes must pass for an engine
change; a unit-test failure does not prevent benchmark qualification from
reporting its own result. The standalone Make target still runs the gate
without the ordinary package/tools tests.
`make probe-executor` times one specification at 10,000, 100,000 and
1,000,000 rows, with and without one keyed intermediate lookup;
`examples/executor_probe.rs` in the `yamaa` crate documents its shapes and
arguments. Record its timings on an issue, never in the repository.

## Writing tests

AGENTS.md states the rules for a test; this section says how each
language keeps them. A test is a claim about one behaviour: an authored
expectation, the smallest input that reaches the behaviour through its
owner's public surface, and one comparison of the complete result. The
expectation is authored, or read from authored truth, and never produced
by running the candidate: the rule for `benchmarks/**/expected/` applies
to every fixture, so no snapshot library and no pasted output enters a
test.

### Shape

- The file's first comment names the family it holds and the rules it
  proves. Each test's name is the sentence it proves, one claim per
  test: a name that needs `and` is two tests or one table. The comment
  above a test cites the rule or decision it enforces (`FILE-11`, `D188
  on #3030`) instead of restating it, and a test that a rule card cites
  as evidence (`Evidence: <path>::<name>`) keeps that name.
- Arrange with the owned factories, act once, then compare the whole
  observable record with an authored literal: a diagnostic is its phase,
  condition, rule, spec paths and context together, including optional
  `source_span` and `operand_route` and whether each is absent; a table
  is its schema and rows; a saved artifact is its bytes. A chain of partial
  checks, or `any(...)` over a list of findings, hides what was
  produced.
- An assertion inside a loop carries the case's label, so a failure
  names the case rather than an index.
- A helper that builds returns a value and asserts nothing; a helper
  that asserts prints the whole actual value when it fails, points at
  its caller (`#[track_caller]` in Rust) and returns what the test needs
  next. Both leave the test body once a second file needs them, and the
  inner modules of one file share one fixture rather than each defining
  their own.
- The input is the two or three rows that reach the behaviour, with
  names and values that make the expectation readable.

### Rust

Integration tests live in `tests/<family>.rs`, each a module of the
crate's one test binary, `tests/main.rs`; the file and its nested `mod`
blocks give a test its path, so a failure reads as
`schema_validation::schema::schema_validation::typed_dictionary_keys_have_independent_key_and_value_paths`.
Families share one process, so a temporary name carries its family's own
prefix. Each crate's `tests/support/` is its one helper tree, declared once
in `tests/main.rs` and reached as `crate::support`. Reach for its factories
before writing a local one: `support::document::Tree` builds a document
by hand, `support::roots` lowers project functions and codelists into
the AST roots a project file produces, `support::ports` (core) holds the
scripted ports that log each effect and panic on an unexpected request, and
the `no_tables!`, `no_raw!`, `no_project_files!` and `no_documents!` halves
of the decoder port for a test decoder that serves only the others,
`support::specification_run` (application) prepares, compiles and lowers a
document and replays authored reports against it, `support::benchmarks`
(application) reads what a live benchmark states (D120),
`support::inheritance::Files` (adapters and application) resolves parents
beside their declaring file, `support::NoFunctions` (yamaa) stands for a
project that calls no function, and `support::Scratch` (adapters,
application and yamaa) is a scratch directory removed on drop. The core,
adapters and application trees hold copies of some of each other's modules
by design (#2158): a change to one is made to every copy, and
`make check-repository` fails while they differ
(`tools.validation.check_test_support`).

- Authored controls are files under `tests/fixtures/`, read with
  `include_str!`; assert their row count so a truncated fixture fails
  (`tests/portable_math_golden.rs` in core).
- A test that writes files or documents, makes one call and compares what
  the call reports is a case of the crate's
  `tests/fixtures/diagnostics/*.json`
  ([Design principles in practice](#design-principles-in-practice)), in
  one case form both crates read (#3691, #3798). A case names the
  behaviour it proves (`case`, unique across the files, which a rule card
  cites as `<file>::<case>`), the operations that reach it (`operation`),
  their `input`, and the complete issue rows they report (`issues`, in the
  shape `assert_issues` compares). Several operations must each report the
  same complete rows, compared with each other whatever `compares`,
  `prefix` and `among` leave out. Core's input holds a specification as
  a JSON object, or a document of the file's `documents` with edits that
  set or remove a member. The application's input holds the `files` its
  operation writes to a fresh study directory, each its text, a
  `documents` entry (`base`) or an authored file named by its path from
  the repository root (`file`), such as a benchmark's `expected/` dataset,
  the last two with edits that each replace text occurring once. Its
  `benchmark` stages a benchmark's files first, each at its path there
  but the `expected/` directory, `README.md` and `benchmark.yaml`, and
  its `edits` map each staged file to its edits; `files` names none of
  them. An issue row's text that starts with `{study}/` names one of
  those files as the engine spells its path. Each crate's
  `support::diagnostic::replay` replays every case once: core's inside
  `diagnostics.rs`'s registry walk, which renders each issue's message,
  and the application's in `diagnostics.rs`. A case of an existing operation
  is data alone; a new operation's reader, and a new fixture file, are
  declared in their module under `support::diagnostic::operations`.
- A positive case also states what its operation built and saved.
  `output` is the output table, whole, written as rows: the header of its
  column names, then one row per record holding one cell per column. A
  case moved from a test whose assertion named just some columns may write
  `"output_subset": true`: its header names those columns in the table's
  order, and it still states every row in order. Without that marker,
  the whole table must match, including its complete header. As with
  `compares` for issue rows, the subset is only for migrated assertions;
  a new case always states the whole table. `report` maps each kind of
  run report row it compares (`run`, `input`, `rows`, `handler`, `column`,
  `check`; CHECK-23) to that kind's rows in report order, written the same
  way, with a header that names the report
  columns it compares in the report's order (CHECK-24) and leaves out
  `KIND`; a kind it does not name is not compared, and a header alone
  states that the report has no row of that kind, so a check of the
  handler counts or the verifications is the `handler` or `check` rows. A
  case names every column of each kind it states; a header that names
  fewer is for a case whose source compared just those columns, as
  `compares` is, and an empty `report` states nothing and is refused. A
  cell is `null` when the value is missing, its text, its integer, `true`
  or `false`, a float in the shortest spelling that reads back as the same
  double, as a diagnostic context spells one (`2.5`, `10.0`, `1e16`), or a
  date or datetime as its ISO text. `saved` states what the operation did
  to the study directory the case staged, whether it was refused or not:
  it maps each file created or changed, by its path in the study
  directory, to its text or to `{"file": path}`, an authored expected file
  named by its path from the repository root, such as a benchmark's
  `expected/` file, and each file removed to `null`; `{}` states that
  nothing was created, changed or removed. A file the operation generated
  whose whole text no authored file holds, such as a composed Define-XML
  document, is instead `{"select": {path: [text, ...]}}`, for a case whose
  source compared parts of it; a document an authored file holds is stated
  whole with `{"file": path}`. Each path selects nodes of the document, and
  its list holds the text of every node it selects, each node whole and
  spelt as the file spells it, in document order, so a count or an absence
  is a list too, `[]` selecting nothing. In an `.xml` file a path is an
  absolute XPath location path of child steps, such as
  `/ODM/Study/MetaDataVersion/ItemDef[@OID='IT.DM.AGE']/def:Origin`: each
  step names an element as the document spells it, prefix included, with at
  most one `[@name='value']` predicate, and the last may be `@name`. An
  element's text runs from its start tag through its end tag, and an
  attribute's is its value between the quotes. In a `.json` file a path is
  a JSON Pointer (RFC 6901), such as `/columns/5/dataType`, and a value's
  text is its JSON text, a string's quotes included. A selected node is
  never matched by a substring, and `select` names at least one path. A
  case states a table only when its operation reports one, and `saved`
  exactly when its operation saves: the
  application's `build` reports the tables a built result holds, `save`
  builds as `build` does, then saves, and `define` composes a project's
  submission section, then saves it, each reporting its issues after the
  save, a save's own refusal included; core's operations report issues
  alone. The members a case may write, and the reading and comparing of
  the positive ones, are `support::diagnostic::form`, one file both crates'
  support trees hold byte for byte. A case the form cannot state yet
  extends `form`, in both copies, rather than becoming a test function.
- A case whose source compared only part of each record names the fields
  it compares in `compares`, and a case whose source compared some keys of
  a context names each as `context.<key>` in place of `context`, its rows'
  contexts holding exactly those keys. A case whose source read the first
  of several issues by position writes `"prefix": true`: its rows are the
  first issues reported, in order, and the issues after them are not
  compared. A case whose source found its record by cause among several
  issues writes `"among": true`: its rows need only be among the issues
  reported, in any order, since the issues its source left unstated give
  its rows no authored position. These are the two exceptions to comparing
  the whole issue list, and a case writes at most one (`form::Order`).
- A variant of a document comes from an exact-once edit,
  `document.edit(from, to)` through the `support::Edit` of the adapters and
  the application, never
  from `str::replace`, which does nothing when its pattern is missing.
  `replace` stays only where every occurrence is meant to change, which
  may be none.
- A file-route run takes its `Identity` from `support::study::identity`,
  its authored issue rows from `support::study::issue_row` and `issues`,
  and a test that only writes files and runs one specification uses
  `support::study::Study`. The corpus tests walk
  `support::benchmarks::all()`, and a test that a benchmark pins calls
  `support::benchmarks::reproduce(name)`, which runs the benchmark as public
  conformance reads it: its first issue against `expected/error.yaml`, or
  every CSV it writes against `expected/`.
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
  code, reaches the shared builders through the crate's `#[path]`
  includes (`crate::test_document::Tree` in core), and repeats nothing
  an integration test proves through the public surface.
- A scratch directory is a `support::Scratch`, named by its label, the
  process and a shared counter, and removed on drop without unwrapping:
  a `Drop` that panics while a test unwinds aborts the whole binary.
  `Scratch::canonical` names it as the engine spells paths,
  `resolved(std::fs::canonicalize)` keeps the standard library's
  spelling, and `adopt` owns a directory nested in another. A test never
  builds a `std::env::temp_dir()` path by hand.
- No `#[ignore]` and no `thread::sleep`. An environment variable serves
  only a declared skip with its reason, as `YAMAA_SKIP_NETWORK_SHARES`
  does, and `Command::new` runs only the program under test: the
  `yamaa-cli` binary, or the shell whose quoting a path test proves.
- Run one family with `cargo test --locked -p <crate> --test main
  <family>::`, and one test with its whole path and `--exact`.

### Python

`tools/tests` is `unittest.TestCase` throughout, run by pytest in
`make test-python` and by `python -m unittest` in `make docs-test`, so a
new file keeps that form: a module docstring naming the family, one class
per subject, `setUp` creating a `tempfile.TemporaryDirectory` registered
with `addCleanup`, and `subTest(case=name)` around each case of a table.
Outside `python/tests/installed/`, `python/tests` is plain pytest with
`tmp_path` and `parametrize`. Fixture text is a module constant or an
authored file; a fixture repository
comes from a builder such as `write_repository` in
`tools/tests/rules_fixture.py`, and the repository root is
`tools.paths.ROOT`. Mock the name the module under test looks up, inside
the narrowest `with`, and spawn a real program only when that program is
the subject, as the staging and command tests do; a new `main` takes
`argv` so its test calls it in-process instead of patching `sys.argv`.
Skip with `self.skipTest(reason)` when an optional tool is missing. A
walk of the benchmark corpus or a `cargo` invocation runs in `setUpClass`
or a cached module function, once per module.
Prefer `assertEqual` on the whole list or dictionary and
`assertRaisesRegex` on the message to `assertIn`, `assertTrue(any(...))`
or a bare `assertRaises`.

The installed suites under `python/tests/installed/` are named
`installed_<area>.py`. The runner stages and executes each file directly
with Python, so its entry point must run its checks. Existing unittest
suites use `unittest.TestCase` and call `unittest.main()` under
`if __name__ == "__main__":`; `installed_public_grammar.py` instead calls
its own `main()` with plain assertions. Preserve each suite's pattern:
pytest-only functions and fixtures are not executed by this runner. These
suites run outside the checkout against an installed package: they import
`yamaa` and the staged `host_fixtures` reader and nothing from `tools/`.

### R

`R/yamaa/tests/*.R` are plain scripts: `R CMD check` runs each in a copy
of the tests directory with the staged inputs beside it, and the first
failing assertion ends the file. The check runs every file
(`--no-stop-on-test-error`), and `tools/commands/check_r.py` prints each
failing file's whole `.Rout.fail`. A script finds its inputs relative to
the working directory and sources the shared readers from
`host-fixtures/` (`reader.R`, `context.R`, and `assert.R` for the
labelled expectations and the helpers the suites share); every shared R
helper lives there, because a file in `rust/tests/host-fixtures/` is
staged beside every suite and into the R source package, while a file in
`R/yamaa/tests/` is run as a suite. Each case is a `local({ ... })` block
that creates its directory with `tempfile()` and removes it with
`on.exit(unlink(dir, recursive = TRUE), add = TRUE)`. Name every
assertion, so a failure says what was expected and for which case, and
compare a whole value with `expect_identical()`, which shows both values
when they differ:

```r
stopifnot(
  "the run keeps both rows" = identical(nrow(table), 2L),
  "the first issue is the refusal" =
    identical(issues$condition[[1]], expected$condition)
)
expect_identical(as.list(issues[c("condition", "rule")]),
  list(condition = "range_failed", rule = "CHECK-7"), "the failed range check")
```

A loop over cases wraps each case in `local()` and puts the case's name
in its labels: `stopifnot_case(label, "what is expected" = ...)` names
the case before the clause. A suggested package that is missing skips the
file with a message, as the files do. A file holds one family, split by
feature, because the first failure hides the rest.

### Shared truth across hosts

A case that Python and R both check is authored once, in
`rust/tests/host-fixtures/cases.json` under a family that its README
lists, with its consumers named in
`rust/qualification/supplemental.json`. Each host's suite reads the
family and asserts the same rows; neither types the expected value by
hand. A host suite adds only what is host-specific: argument conversion,
retained ownership, i64 and NUL handling, callbacks, interrupts and the
host's own failure text. The command line's tests
(`rust/crates/yamaa-cli/tests/parity.rs`) compare its `--format json` issues
and saved run reports with the same issue and run-report parity cases, so a
case whose rows or files differ by host gives them under `python`, `r` and
`cli`.

## CI and release qualification

The ordinary CI workflow selects from the same checks listed above for each PR,
every two hours on main, and on each push to a `release/**` branch. Merges to
main do not start workflows other than the documentation site deployment; main
is checked on those schedules instead. While the newest main run of CI, the
documentation deployment, or a weekly lane below (`Dependency graphs` and
`Next toolchain`) fails, the `Main health` workflow keeps a `main-red`
issue open for it and closes the issue when a later main run succeeds. A
failing fuzz run on main opens such an issue, or comments on the open one, and
a person closes it: a later green run does not show that the crash is fixed,
because the next run may not find the same input again. A PR that changes
`.github/` also runs actionlint over every workflow in the repository lane.
Keep workflow-specific setup in CI and reusable check logic in `tools/`.
Pin external actions to complete commit IDs with exact release comments.

PRs report four required checks: `repository`, `rust`, `python` and `r`.
Every lane checks out the exact head and selects work from the merge base
before installing tools. An invalid diff or a wrong checkout fails the lane.
The repository lane runs Python formatting/type checks and repository
validation; Rust formatting and Clippy run beside the Rust tests. Minimum
Rust lint runs at the end of the Rust lane, after the pinned-toolchain tests,
for Rust/build changes. The two host lanes run their tests and any selected
equivalence on a single fresh installation each.

A documentation-only PR validates rule links, documentation prose and site
YAML examples, then runs `make docs-test docs`, with locked tooling and
no installed host or Rust toolchain. The other three checks report a skip
notice without installing their tools. Known repository-only tools run the
repository and Python checks. A host-only change runs the affected host;
shared Python dependency metadata, engine/build changes and unknown paths
run all four lanes. Fuzzing remains a separate workflow selected by affected
parsers. There is no separate reporting job or equivalence status to wait for.
The main schedule and release pushes run all development checks; dispatch
also qualifies both hosts against benchmark truth.

The `Release compatibility` workflow checks the minimum Rust toolchain, the R
source package built with it, minimal format builds, Python 3.15 and minimum
Python saved artifact readers on each `release/**` push and on dispatch. CI's R job checks minimum R saved
artifact readers on those same release events. The `R-CMD-check` workflow
checks macOS, Windows and R-devel packages on release pushes and dispatch.
Dependency graphs and advisories, Harbor and coverage run on dispatch
(dependency graphs and advisories also weekly on main). Dispatch any of them
on a branch with `gh workflow run <workflow> --ref <branch>` to check a change
that needs them before merging. Native qualification, described below, runs
only in the regular CI/CD on main: weekly, and at the candidate and release
cuts (D233 on #3673). Never dispatch it on a pull request's branch or push
that branch as `qualification/issue-*`; no pull request waits for a native
run. A pull request that changes packaging, paths, file handling,
`cfg`-gated code or a workflow says so in its body: the next native run on
main is its native evidence.

The `Fuzz` workflow fuzzes the readers that parse untrusted input under
AddressSanitizer, with UndefinedBehaviorSanitizer for the C they compile. A PR
runs, for three minutes each, the targets whose readers it changes; a nightly
schedule runs every target for thirty minutes. The repository checks also run
Clippy over the targets in the Rust lane, so a change that breaks one fails
on its PR. The [fuzz guide](rust/fuzz/README.md) lists the targets and runs them locally with
`python -m tools.ci.fuzz`. Fix a crash with a unit test in the owning crate and
commit its input as a text regression beside the targets.

At the end of CI's Rust lane, Clippy with `RUSTFLAGS=-D warnings` checks the
workspace with its declared `rust-version`, Rust 1.82.0, instead of the pinned
toolchain that the other checks use, because embedders
and the source packages build with their own toolchains, and the pinned
toolchain accepts code that the minimum refuses. It runs for Rust/build
changes and on the main schedule, as Cargo's guide to continuous integration
asks for a declared `rust-version`. It leaves the R binding out, as the
repository lint does: the binding's build script runs R, and only the jobs
that check the R host install R. The `Release compatibility` workflow's
`rust (minimum toolchain)` job repeats that lint with the R binding, after
`cargo check`,
on release branches and adds the builds below, and native qualification
builds the R source package with the minimum. Rust 1.82.0 was released on
2024-10-17, so it meets CRAN's request to test with a cargo at least two
years old on any submission date.
Run the lint locally from `rust/`; with R installed, drop `--exclude yamaa-r`
to cover the R binding as the release lane does:

```sh
rustup toolchain install 1.82.0 --profile minimal --component clippy
RUSTFLAGS='-D warnings' cargo +1.82.0 clippy --locked --workspace --exclude yamaa-r --all-targets --all-features -- -D warnings
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
These two builds compile the adapter's unit tests, run only the focused
adapter capability tests, verify the facade's dependency graph and linked
native symbols, and check the core's dependency isolation once. Ordinary
workspace tests cover all default formats.
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
minimum and newest compatible graphs, on dispatch and weekly on main: a new
advisory, a lapsed ignore or a new release of a dependency changes what the
same locks and manifests prove without a commit, so the weekly run, which
`Main health` reports, finds it before a release does. Run a lane locally
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

Before every merge into `main` or a `release/X.Y.Z` branch, bring the branch up
to date with its base and wait until CI is green on the exact head, including
the affected host lanes' equivalence checks. Never merge while a required job
is queued, running or cancelled (D176 on #3030). Then run a fresh read-only
review check with an authenticated GitHub CLI, supplying the exact final
commit:

```sh
python -m tools.ci.review_gate --repository elong0527/yamaa --pr <number> --head <full-commit>
```

Use that command when GitHub CLI API access is available. If environment policy
blocks it, approved authenticated GitHub connector reads must prove every same
predicate from fresh API evidence: the exact head and base, this PR's authentic
completed status, its walkthrough and final reviewed footer, no review in
progress, unchanged base history, and all resolved threads. Re-read the PR
head and status immediately before merging. Large PRs require the complete
bounded proof below through those reads as well. An unavailable CLI does not
waive review requirements.

The merge needs one completed review of the exact head and every thread
resolved (AGENTS.md, "Complete review before merging"). The review is
CodeRabbit's, checked by the command above; Codex's completed review of that
head, when CodeRabbit has not reviewed it; or the maintenance session's own,
recorded on the pull request as `## Review of <full sha>`. The rest of this
section states the CodeRabbit path's evidence. It requires CodeRabbit's
authentic `Review completed` status on that commit and resolved review threads. The PR must have its own authentic summary and
walkthrough, with a final reviewed footer naming that exact head and no review
in progress. A later edit of a finished summary, such as an offer to fix
failing CI, does not start a new review. The reviewed range must end at that
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
CodeRabbit cannot review the PR (its free reviews are exhausted, or it skips
the PR). Merging then does not wait for it: the owner's maintenance session
(D175 on #3030) records its review of the exact diff on the PR as
`## Review of <full sha>`.
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
formal review for that commit. Completion must follow the formal review's
submission when that receipt is used. Missing evidence, another PR sharing the
head, or any recorded
base retarget/force-push history requires a fresh review PR and head. This
prevents a completion for an earlier base from covering a different diff.
The gate checks complete Git trees and paginated PR files through the authenticated API,
then rechecks all PR heads, bases, batch metadata and review statuses. It fails
on truncated trees or incomplete evidence. Moving the target source requires
new matching batch heads and fresh completed reviews. Batch metadata belongs
in the PR body, not a committed evidence ledger.

Native qualification is separate: `.github/workflows/native.yml` builds and
installs the supported Python wheel/source and R source and binary
combinations on dispatch, on a `qualification/issue-*` push and weekly on
main. It checks public benchmark conformance, installed host suites,
packaging, optimized Rust behavior and portable math, installs each wheel
that a release publishes from local files with the network disabled (its
`native-offline-wheel` jobs), installs both source archives that a release
publishes with the network disabled on every platform (its
`native-offline-source` jobs) and again on Linux with the minimum Rust,
installed from its standalone installer with no rustup or network (its
`native-offline-toolchain` jobs), builds the R binary packages that a
release publishes and installs each with no Rust, Rtools or network (its
`native-r-binary` and `native-r-binary-install` jobs), and runs CRAN's
incoming checks on the R source archive that a release publishes (its
`native-cran` jobs), also in R-hub's `gcc16` container, in its `gcc-asan`
and `clang-asan` containers, whose C compilers instrument the package's C
with AddressSanitizer and UndefinedBehaviorSanitizer, and in its `valgrind`
container, where the examples and tests run under valgrind's memcheck. It
also builds the
command-line binaries that a release publishes, statically on Linux and
Windows, and runs each archive's `yamaa selftest` and then the conformance
corpus with the network disabled on a fresh runner of its platform (its
`native-cli` and `native-cli-install` jobs), and runs the corpus with the
installed Windows wheel on an SMB share of the runner's own, through its UNC
path and through a drive mapped to it (its `native-share` jobs). Passing
ordinary CI does not qualify a release. The final
merged revision needs complete native evidence, including the R package
checks below. A release tag publishes the native run that its
`Qualified-Run` trailer names, and the `Release rehearsal` workflow, dispatched
before tagging, runs the same selection against the latest successful main run
without publishing;
see [publication](docs/release.md#publication). After each PyPI upload, the
`PyPI install` workflow, dispatched with the release's Python version, installs
it from PyPI on each wheel platform and runs its selftest; see
[PyPI upload](docs/release.md#pypi-upload).

Every R package check, in CI's `r` job (`make test-r`), native
qualification, the `R-CMD-check` workflow and the `r (minimum toolchain)` job,
runs `R CMD check --as-cran` through `tools/commands/check_r.py`, with the
settings of CRAN's own checks that `--as-cran` leaves off. It fails on any NOTE, WARNING or ERROR that
`R/yamaa/cran-comments.md` does not explain: its `EXPECTED_NOTES` lists the
explained notes with the lines each may print, and a test keeps that list
equal to the notes that the file's "R CMD check results" section names. To
accept a new note, explain it there and add it to `EXPECTED_NOTES` in the same
pull request; a warning or an error is never accepted. With an R whose
compiler adds the sanitizers, as in the `native-cran (gcc-asan)` and
`native-cran (clang-asan)` jobs, it also fails on any AddressSanitizer or
UndefinedBehaviorSanitizer report in the installation, example or test
output, which it prints whole: R passes a test that UndefinedBehaviorSanitizer
reported on. With `--valgrind`, as in the `native-cran (valgrind)` job, it
runs CRAN's valgrind check instead, as R-hub's image does: only the
installation, the examples and the tests (`--extra-arch`), the last two under
valgrind's memcheck (`--use-valgrind`), without `--as-cran`, which would turn
`--extra-arch` off and report every example as slow. It fails on any output of
the examples or a test where memcheck counted an error, which it prints whole,
or did not run. It adds `tools/commands/memcheck.supp` to the image's
`VALGRIND_OPTS`, which hides only the possibly lost blocks of three roots
that live as long as the process, and first runs
`tools/commands/memcheck_control.c` under valgrind to prove that every other
leak still counts. Remote checks (CRAN's
package index, URLs and the clock) stay off unless the caller sets
`_R_CHECK_CRAN_INCOMING_REMOTE_` or `_R_CHECK_SYSTEM_CLOCK_`. The PDF and
HTML manuals, which need LaTeX and HTML Tidy, stay out (`--no-manual`)
unless the caller passes `--manual`, as native qualification's
`native-cran (ubuntu-24.04, R devel, manual)` job does after installing
both; the check then fails unless R reports both manual checks OK. The Linux jobs
have `checkbashisms`, and those of CI, native qualification and R-CMD-check
also `aspell`, so the check covers the configure scripts and the spelling of
`DESCRIPTION` as CRAN's does; elsewhere R warns that it lacks
`checkbashisms`, and that one warning passes.

The R source archive carries a small base-R smoke suite and its project
study. It runs the embedded format selftest, public specification/project
calls, R callbacks and saved-byte checks without suggested packages. The full
R suites and fixtures remain in the repository: `make test-r` and the R
pull-request lane stage every cataloged suite outside the checkout and run
it against the fresh source installation before the separate CRAN check.
Native qualification continues to run the full installed suites against its
source and binary artifacts.

Source-package builds also render the minimal introduction under
`R/yamaa/vignettes/`, with `knitr`, `rmarkdown` and Pandoc installed. Its
short example is self-contained and is not evaluated during rendering.
When editing it, run the example against the installed package as well as
building the vignette. The source tarball includes the rendered HTML.

`tools/staging/vendor_exclusions.toml` is the version-specific allowlist of
upstream development directories omitted from source bundles. Audit a new
entry for build-script and `include!`/`include_str!`/`include_bytes!` inputs,
keep every needed license and production file, and prove offline production
builds with the pruned bundle. Unknown versions keep their development files.

The full R suites use the package's suggested packages, `arrow` and
`jsonlite`, only where they are installed, as "Writing R Extensions" asks:
a part that needs a missing one is skipped with a message. Native qualification's
`native-cran (ubuntu-24.04, R devel, no Suggests)` job installs no Suggests, as
CRAN's noSuggests check does. It sets `_R_CHECK_DEPENDS_ONLY_=true`, so the
smoke checks see only the Depends and Imports, and `_R_CHECK_FORCE_SUGGESTS_=false`,
so R reports the missing packages instead of stopping. It passes
`--ignore-vignettes` to the owned checker because knitr and rmarkdown are
absent too; the other CRAN checks rebuild and validate the vignette. R-hub
containers install every package in DESCRIPTION's Suggests, and Pandoc.
For a local dependency-only check, build the archive with the vignette tools
available, then select libraries without suggested packages:

```sh
python -m tools.staging.stage_r_package /tmp/yamaa-r-no-suggests/yamaa
cd /tmp/yamaa-r-no-suggests && R CMD build --no-manual yamaa && mkdir library
R_LIBS="$PWD/library" R_LIBS_USER="$PWD/library" R_LIBS_SITE="$PWD/library" _R_CHECK_DEPENDS_ONLY_=true \
  _R_CHECK_FORCE_SUGGESTS_=false python -m tools.commands.check_r \
  --ignore-vignettes --archive yamaa_*.tar.gz --work-dir check
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

The YAML examples that readers copy are checked against their file types'
JSON Schemas. `make check-repository` reads each fenced `yaml` block of the
site pages (`README.md`, `rules/README.md` and `docs/articles/*.md`), of the
package READMEs and of the issue forms' placeholders
(`tools.validation.check_site_examples`). A block whose root declares `yamaa:`
is a whole 1.0 file: it must parse, name its `type:` and validate against that
type's schema. A block without `yamaa:`, such as an excerpt of a few fields, is
not checked. A block that declares `yamaa:` but shows only part of a file
starts with the line `# fragment`, and the check skips it.

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
