# Release and migration

This page records what a yamaa release supports, the behavior decisions it
keeps, and how to migrate older specifications. Release status, evidence and
open gates live in GitHub issues and pull requests, not here.

## Supported platforms

- CPython 3.12, 3.13, 3.14 and 3.15 wheels and source builds on Linux
  x86_64, macOS arm64 and Windows x64. The Linux wheel is `manylinux_2_34`
  and needs glibc 2.34 or later, for example RHEL 9, Ubuntu 22.04 or
  Debian 12; older systems build from the Python source archive.
- R 4.6.1 source builds on the same three hosts, each passing strict
  `R CMD check` with **Status: OK**.
- Other runtimes and architectures, free-threaded Python, musl and R binary
  packages are not supported. R is distributed as a GitHub source tarball;
  CRAN and r-universe are later work.

Source builds compile the Rust workspace, so they need Rust 1.90.0 or later
with Cargo, a C compiler and GNU make. Cargo downloads the locked crates from
crates.io, and pip's build isolation fetches `maturin` 1.9.6. An offline build
needs those crates already in the Cargo cache; an offline Python build also
needs `maturin` installed for `pip install --no-build-isolation`. Windows R
builds also need Rtools45 and the GNU Rust target on the pinned toolchain, as
[Supported dependency combinations](#supported-dependency-combinations)
describes.

`.github/workflows/native.yml` builds, installs and tests every supported
combination on manual dispatch. A release is cut from a commit whose complete
native run passed. Development PRs do not run the release lanes, so before a
release also dispatch R-CMD-check, Optional format features and Dependency
graphs on the candidate, and CI for its minimum toolchain, Python 3.15 and
minimum saved artifact reader lanes.

Python, R and Cargo versions move together;
`tools.packaging.check_shared_release_versions` checks that they agree. The
previous Python release is the `yamaa-py-0.3.0` tag.

## Saved artifact readers

The minimum supported independent readers are:

| Reader | Minimum | Saved formats qualified |
| --- | --- | --- |
| Python Polars | 1.35.2 (2.0.0 on CPython 3.15) | CSV text and Parquet |
| Python PyArrow | 23.0.1 (26.0.0 on CPython 3.15) | CSV text and Parquet |
| R arrow | 23.0.1 | CSV text and Parquet |
| Python standard library | Supported CPython above | Dataset-JSON 1.1 |
| R jsonlite | 2.0.0 | Dataset-JSON 1.1, with `bigint_as_char = TRUE` |
| Python lxml | 6.1.3 | Define-XML 2.1 |

Python package dependencies and R package `Suggests` declare these floors.
The R readers are optional: public Build and Save use the shared native codecs.
CSV carries text rather than an embedded schema; controls supply an explicit
text schema and check exact authored CSV bytes. Its empty-field ingestion
semantics are described in the [CSV profile](../rules/reference/csv.md).
Parquet checks every logical type, nullable field, zone-free microsecond
timestamp, field and row order, full signed-i64 values, exact finite double
bits and empty-table schema. Read R integer columns through Arrow arrays, or
cast to text inside Arrow: converting to `bit64` loses `INT64_MIN` to its
missing-value sentinel. R doubles cannot represent every signed-i64 integer.

Dataset-JSON controls use column metadata to retain integer, float and temporal
meaning. Python's integer decoder must preserve the JSON token `-0` when it
belongs to a float column. jsonlite decodes that token as integer zero; R
qualification checks its mathematical value and the original signed token,
and pins every other finite float's bits. CSV ingestion maps collected empty
text to missing; Parquet retains it as present empty text. Both forms are
covered before Dataset-JSON publication. Define-XML has independent schema and
fixed-byte controls in the installed public Define suites.

The standalone `saved_readers` suites use shared independently authored
fixtures under `rust/tests/host-fixtures/`, build through installed Python and
R public APIs, remove the original inputs and specifications, then read the
held Save bytes. The Python readers decode the same captured byte buffer;
R arrow reads its installed host's corresponding captured buffer. Repeated
saves of one run must retain identical bytes. Ordinary CI tests the minimum
readers; every native wheel/source and R source lane also runs these controls
with the minimum versions for its interpreter. SAS datasets and Transport are
input formats; the published dataset profiles remain CSV and Parquet. SAS
Transport version 5 and Dataset-JSON files and Define-XML are generated through
the public project submission API.

## Supported dependency combinations

The Rust workspace declares MSRV 1.90 and builds with Rust 1.99.1, the
toolchain that `rust/rust-toolchain.toml` pins; a CI job checks and lints it
with Rust 1.90.0. The R source package runs Cargo inside its bundled
workspace, so under rustup it builds with the pinned toolchain and ignores a
Rust target added to another one: Windows R builds need
`rustup toolchain install 1.99.1 --target x86_64-pc-windows-gnu`. Canonical
release builds use `rust/Cargo.lock`, including unchanged copies in both source
packages. Arrow and Parquet are supported at exactly 57.3.0 with flatbuffers
25.12.19, so an embedder whose graph holds Arrow 57.3.0 builds the engine
unchanged. Other Arrow and Parquet releases are unsupported: the adapter's
version-specific APIs, confined to `storage_api.rs`, are adapted and qualified
for 57.3.0 only. Broadening that range requires separate adapter and
installed-host qualification first. The direct storage codec floors are brotli
8.0.4 and lz4_flex 0.13.1. Portable libm remains exactly 0.2.16. The reviewed
saphyr-parser 0.2.1 with its compact-flow fix is vendored as
`yamaa-saphyr-parser`, which `yamaa-adapters` depends on by path. It is
therefore part of every graph that builds the engine, including a project that
depends on the facade by git or path with no other configuration; CI's
minimum-toolchain release lane builds the facade from such an outside
workspace (#2929). The Python
binding builds with PyO3 0.27.2 through 0.30; the lock selects 0.30.1, and the
minimum dependency graph tests 0.27.2.

The engine deliberately links two versions of four crates, because yamaa's
direct floors differ from Parquet 57.3.0's: lz4_flex 0.13 beside Parquet's
0.12, zstd 0.14 beside 0.13, base64 0.23 beside 0.22, and num-bigint 0.5
beside the 0.4 that Parquet and arrow-buffer use. Aligning them would lower
yamaa's floors or move Parquet off its exact pin; Parquet 57.3.1 requires the
same versions as 57.3.0. Both zstd copies, through zstd-safe 7 and 8, build
one `zstd-sys`, so the engine links a single zstd library. The other duplicate
versions that `cargo tree -d` lists add no second linked copy: syn serves
procedural macros, and the extra getrandom and r-efi versions serve build
tools or other targets.

Python supports CPython 3.12 through 3.15 with the runtime ranges in
`python/pyproject.toml`. On CPython 3.12 to 3.14, their declared floors are
lxml 6.1.3, Polars 1.35.2, py-yaml12 0.2, PyArrow 23.0.1, Pydantic 2.12.4,
packaging 25 and PyYAML 6.0.3.
The Parquet 57.3.0 writer records only type-defined column orders, which
Polars 1.35.2 reads; the IEEE 754 total-order form that later writers record
for floating-point columns needs Polars 1.43.2. R uses R 4.6.1 and the shared
Rust graph; its host qualification readers are the `Suggests` dependencies in
`R/yamaa/DESCRIPTION`.

Three of these floors do not work on CPython 3.15, so on 3.15 the declared
floors are the oldest releases that do: Polars 2.0.0, PyArrow 26.0.0 and
Pydantic 2.14.0. Environment markers split these three requirements by
interpreter, and CPython 3.12 to 3.14 keep the floors that embedders hold.
Polars before 2.0.0 installs, but its Series methods that forward to
expressions, such as `implode`, return `None`: 3.15 no longer stores a
function's implicit `None` return in `co_consts`, which those releases use to
find the methods. PyArrow 23.0.1 and pydantic-core 2.41.5, which Pydantic
2.12.4 requires, publish no 3.15 wheels, and their source builds need Arrow C++
or a PyO3 release that accepts 3.15. PyYAML publishes no 3.15 wheels either,
but builds from its source archive, so its floor is the same on every
interpreter. So is lxml's: 6.1.3 is the first release that loads no external
parameter entities by default, and 6.1.1 patched the libxslt that its Linux
wheels bundle.

The [dependency graph lanes](../CONTRIBUTING.md#ci-and-release-qualification)
test declared direct minima and newest compatible selections on Linux with
CPython 3.12, 3.14 and 3.15, all with R 4.6.1; on 3.15 the minima are its own
floors. Cargo transitive dependencies use the newest
MSRV-compatible resolution in both lanes; vendored crates and exact pins are
preserved. Python runtime/test floors are exact in the minimum lane, while
Python transitives are resolved compatibly. These isolated graphs do not change
`rust/Cargo.lock` or `python/uv.lock`. The full native matrix separately qualifies
the locked release across supported platforms and Python versions.

The same workflow checks both locks against security advisories.
`cargo deny check advisories` reads `rust/Cargo.lock` against RustSec with
`rust/deny.toml`, `pip-audit` reads every package locked in `python/uv.lock`
against the PyPI advisory database, and OSV-Scanner reads both locks against
OSV with `osv-scanner.toml`. OSV also carries GitHub's reviewed advisories,
which the other two databases can lack. An ignored advisory names its id, a
reason and a review date in the file of each scanner that reports it. An
OSV-Scanner ignore lapses on its review date, so the check fails again until
the advisory is reviewed.

### Components outside the advisory scanners

The scanners match locked registry packages by name and version, so they do
not cover vendored source or native libraries bundled inside a package. These
components are tracked by hand:

- `saphyr-parser` 0.2.1 with the reviewed compact-flow patch, vendored as
  `yamaa-saphyr-parser`, a path dependency in `rust/vendor/saphyr-parser`.
  #2583 replaces it with a published release.
- The ReadStat 1.1.9 SAS reader subset in
  `rust/crates/yamaa-adapters/vendor/readstat`. #2927 tracks the upstream
  fixes since 1.1.9.
- zstd 1.5.7, the C library that `zstd-sys` bundles into the engine.
- libxml2 and libxslt, bundled in the `lxml` wheels. The lxml 6.1.3 Windows
  wheels bundle libxml2 2.11.9, an end-of-life branch that lxml describes as
  security-patched.

## Release checklist

Before pushing the release tag, confirm on the final commit:

1. Its complete native qualification passed.
2. Its Dependency graphs run passed, including the three advisory checks.
3. Every ignored advisory in `rust/deny.toml` and `osv-scanner.toml` still
   matches a locked package, and its reason still holds. Remove any ignore
   that fails either test.
4. Every [component outside the advisory scanners](#components-outside-the-advisory-scanners)
   was compared with its upstream releases and security advisories since the
   previous release, and each fix that applies is taken or has an issue.
5. A [release rehearsal](#publication) passed against a native run of that
   commit; the tag names that run.

Record the outcome on the release's planning issue.

## Publication

After the final commit is on main and its complete native qualification has
passed, push the coordinated tag `yamaa-v<version>` at that exact commit. The
tag must be annotated, and its message must end with a trailer paragraph that
names the qualifying native run as `Qualified-Run: <run id>`. The run id is the
number at the end of the run's URL, also listed by

```sh
gh run list --repo elong0527/yamaa --workflow native.yml --commit <commit> --status success
```

Tag and push:

```sh
git tag -a yamaa-v<version> <commit> -m "yamaa <version>" -m "Qualified-Run: <run id>"
git push origin refs/tags/yamaa-v<version>
```

`.github/workflows/release.yml` publishes from the named run alone, so a later
run of the same commit cannot replace the run the release was reviewed against.
The named run must be a completed successful native run of the peeled tag
commit: a scheduled main run, a main dispatch, or a same-repository
`qualification/issue-*` push. It requires all installed lanes, shared
Rust/math, complete cohort comparisons and strict R package checks to have
passed. Branch dispatches cannot release. A lightweight tag, a missing,
repeated or malformed trailer, or a named run that does not qualify fails
before anything is downloaded.

The workflow downloads the qualified artifacts and publishes their unchanged
bytes: one wheel per supported OS, the Linux-built Python sdist and the
Linux-built R source tarball. Every native source lane, on Linux, macOS and
Windows, installs and tests those same two archives. Missing, expired or
ambiguous artifacts fail publication; an existing
release is never overwritten. Re-running failed jobs or all jobs keeps a run
eligible: earlier attempts' artifacts stay on the run, and the workflow takes
only the copy that its uploading job created during the execution the run's
final attempt lists. The release body is the committed notes from
`docs/releases/<version>.md`, then a "Qualified native artifacts" section that
links the native run attempt and lists the selected artifacts.
This route uses the workflow's GitHub token and needs no package-index
credentials. The workflow has no PyPI step: 0.4.0 is distributed as GitHub
release assets only, and those assets do not establish PyPI publication. PyPI
publication is a separate decision and requires configured authentication;
the GitHub release assets remain the documented standalone download route.

The `Release rehearsal` workflow runs the same selection and downloads, on
dispatch, against the latest successful native run on main, with its own
read-only token. It reads no tag, does not look for an existing release and
stops before `gh release create`. Dispatch it before tagging with
`gh workflow run release-rehearsal.yml --repo elong0527/yamaa`; its log names
the run it rehearsed. To rehearse another run, check out that run's commit,
fetch `origin/main`, and run the selector with an authenticated GitHub CLI:

```sh
python -m tools.ci.select_release_artifacts --repository elong0527/yamaa \
  --run-id <run id> --destination <new directory>
```

## Source package contents

Python and R source packages carry the same Rust workspace, static ReadStat
bridge and patched YAML parser. SAS and Transport decoding runs in the shared
adapter over captured source bytes; it needs no host reader fallback or dynamic
ReadStat installation. The [ReadStat integration](../rust/crates/yamaa-adapters/vendor/readstat/README.md)
and [YAML parser provenance](../rust/vendor/saphyr-parser/PROVENANCE.md)
own their source inventories, patches and licenses. Both parser authors' MIT
notices remain in the distributed `LICENSE-YAML.txt`.

Both source packages carry `rust/rust-toolchain.toml` unchanged. It names Rust
1.99.1 in rustup's minimal profile and no other component, so a source
installation never adds Clippy or rustfmt. The R package runs Cargo inside its
bundled workspace, so under rustup it builds with Rust 1.99.1, as its
`DESCRIPTION` states; a build without rustup needs Rust 1.90 or later. pip
builds the Python source archive from its root, outside that declaration, with
the caller's default toolchain, which must also be Rust 1.90 or later.

The installed R package carries only the test inputs that its installed suites
read, and each third-party SAS fixture keeps its license beside it.
`tools/staging/stage_r_package.py` lists the staged inputs it leaves out.

Every package carries the project license and the third-party notices: each
wheel in its `.dist-info/licenses/` directory, the Python source archive at its
root, and the R source package in `inst/`, which R installs at the package
root. `LICENSE.txt` is the project's MIT license. `LICENSE-THIRD-PARTY.txt`
reproduces the license, copyright and NOTICE texts of every third-party crate
in the normal and build dependency closure of the shipped Python and R
binding crates, for every platform, and names the license relied on where a
crate offers a choice. It also reproduces the notice that the Rust project
ships for the standard library of the pinned toolchain,
`share/doc/rust/COPYRIGHT-library.html`, because the native extension links
that library. `LICENSE-READSTAT.txt`,
`LICENSE-UNICODE.txt` and `LICENSE-YAML.txt` cover the vendored ReadStat
sources, the Unicode data in yamaa's own tables and the vendored YAML parser.
The R package keeps its `MIT + file LICENSE` declaration, and its `Copyright`
field names these files. The Python License-Expression,
`MIT AND Apache-2.0 AND BSD-3-Clause AND Unicode-3.0`, covers the project, the
crates linked into the wheels and the standard library, which is relied on
under MIT, with Unicode-3.0 for the Unicode tables in `core`.

`tools.packaging.generate_third_party_notices` generates the crate notices
from `rust/Cargo.lock` and the standard library section from the toolchain
that `rust/rust-toolchain.toml` pins, and it derives that expression. The
repository check fails when the locked graph gains or loses a crate without a
matching notice entry, when `rustc` in `rust/` is not the pinned toolchain, and
when the committed section differs from that toolchain's notice.
Native qualification and the release selector require every wheel and source
package to carry each notice unchanged.

Native qualification exercises the static source bridge on every supported
platform and compares both installed hosts with independently authored binary
fixtures and exact saved bytes. The [SAS source contract](../rules/reference/sas.md)
owns decoding behavior.

## Approved decisions

- Export Python `spec`, `project`, `read_odm`, `Error` and `__version__`, and
  R `yamaa_spec`, `yamaa_project` and `yamaa_read_odm` (#2967). A spec checks,
  resolves and runs one specification; a project selects its listed
  specifications by `domain` name, checks them all, runs one section or all
  sections, and composes submissions. Only results write files, through
  `save()`. `resolve()` returns the composed specification as a dict or named
  list. Legacy IO/authoring extensions and `cdiscbuilder` are retired. The
  style CLI remains private and native-backed.
- Keep wrong argument types as `TypeError` in Python; in R they are ordinary
  errors. `check()` and `run()` return specification, data and project
  problems as issue rows, missing specifications included, and never raise for
  them; `save()`, `resolve()` and an unknown specification name or section
  raise `yamaa.Error` or a `yamaa_error` condition carrying `issues`. Check
  returns its findings without reading study data. The project file links each
  host's environment file; no call takes an environment or a resource
  resolver.
- Return results as owned host tables: Polars DataFrames in Python and
  data.frames in R. No host exposes Arrow views.
- Keep the Rust adapter features `yaml`, `parquet`, `readstat` and `odm`
  independent, all enabled by default; the Python and R packages enable all
  four.
- Preserve separate missingness and preserve-or-error conversions: Python
  keeps full signed-i64 values, and R uses plain doubles and character in
  output and in project function calls, raising on an integer beyond 2^53 or
  text containing NUL. R has no exact-value classes.
- Report every finding in one issues table, built in Rust and identical in
  both hosts: `phase`, `condition`, `requirement`, `spec_paths`, `context` as
  canonical JSON text, and the optional `source_span` and `operand_route`. No
  host interprets conditions, and there are no separate diagnostic records.
- Give each run one run report, written to `output.report`, in place of the
  verification and warning logs (#2912).
- Resolve resources from their original declaring file by default; explicit
  input/output `relative_to: entry` selects each specification entry. Admit
  absolute and parent-relative local paths without project-root fallback. Preserve snapshots,
  original failures, callback counts and log-first publication order. A `schema`
  link types a stored artifact from its producing specification and never runs
  it; callers build and save producers before their consumers.
- Keep normative diagnostics registry-owned, including the reviewed
  `unsupported_operation` and `engine_rejected` null-requirement exceptions.
  Cut breaks remain finite and strictly ascending; signed-i64 predicate
  overflow, CSV empty-string refusal, greatest/least first-source ties and
  row-filter behavior follow the normative rules and expected benchmark data.
- Use PortableLibmV1 and pinned libm 0.2.16. EXP/LN/POWER results, downstream
  predicates, conversions and saved bytes may differ from historical platform
  math. This promises neither universal correct rounding nor historical bit
  compatibility. The golden values live in
  [portable_math.tsv](../rust/crates/yamaa-core/tests/fixtures/portable_math.tsv).
- Keep canonical Cargo locking and unchanged source-package copies. Runtime,
  contract, cache and resource identities use references, canonical forms or
  bytes; only packaging-tool lockfiles carry their own checksum fields.
- Prune every specification after composition, including an entry file that
  declares no `parent`; resolved-column ordering stays inheritance-only.
- Ship `str_replace` with literal replacement text only. Group references,
  if ever added, come through a different field, so `replacement` never
  changes meaning.
- Promote row defaults per template: a column can be row-phase for the
  templates that need it and column-phase for the rest, and a candidate a
  template filter suppresses never reads a lookup it does not need.
- Pin Arrow and Parquet to exactly 57.3.0, with brotli 8.0.4 and lz4_flex
  0.13.1 floors, so embedders on Arrow 57.3.0 build the engine unchanged. The
  57.3.0 writer's type-defined column orders return the Python Polars floor to
  1.35.2, with no vendored Parquet writer.
- Ship the reviewed vendored `saphyr-parser` compact-flow patch in every
  supported graph; replacing it with a published parser is 0.5.0 work.
- Keep Cargo dependency floors where the minimum dependency-graph lane tests
  them; the crates are not published.
- Generate both hosts' binding declarations from
  `rust/interface/bindings.yaml` and compare them in the repository check
  and the native qualification lane. The version and feature handshake
  runtime waits for an engine that loads separately from the host facade.
- The `yamaa` facade crate is the only supported Rust surface;
  `yamaa-adapters` and `yamaa-core` are internal and may change in any
  release.
- Distribute 0.4.0 as GitHub release assets only; `release.yml` has no PyPI
  step.
- Read only the live records of a SAS7BDAT dataset that holds records
  deleted in place, and refuse it as `source_sas_invalid` with cause
  `deleted_records` when its count of deleted records and the records marked
  deleted disagree (#2924, #2927).
- A qualified `between.value`, in a named intermediate or an aggregate, reads
  the record of the input that drives the row. Any other input fails
  validation as `unknown_field`; it is never read through the implicit join
  (#2925).

The [R and Python API](articles/api.md) page describes host behavior. The
[Python README](../python/README.md#installation)
and the
[R README](../R/yamaa/README.md#installation)
give the installation commands.

## Resource refusals

In the shared engine, a fixed resource boundary produces `engine_rejected` with
a null requirement and its authored specification paths. The context retains the lifecycle stage, the
boundary's code, its resource name and integer limit, and the observed `actual`
size when available. Compiler boundaries use `compiler_limit`; physical capture,
preparation and execution boundaries retain their corresponding resource code
and phase. Static Check and Build share the same preparation facts.

A refusal stops that operation before its later effects. Earlier completed
findings and original source provenance remain available. Static Check retains
its metadata-only authority. Reduce the refused size and rerun the operation.

## Resource limits

The shared Rust engine behind every Python and R spec and project call uses one
fixed policy in `yamaa_core::limits::Limits`.
Admission checks the physical file, table geometry, declaration shape or compiled
regex at its owning boundary. There are no caller-selected profiles or
cumulative byte, copy, traversal or work pools.

`read_odm()` keeps its own archive-member, expanded-byte and XML-depth options
in both hosts and reports a parser refusal as `yamaa.Error` in Python or a
`yamaa_error` condition in R.

| Boundary | Fixed maximum |
| --- | ---: |
| Each specification, schema, project, environment, function, codelist or packaging-lock file | 8 MiB |
| Each encoded study source file | 10 GiB |
| Each saved output file | 10 GiB |
| Rows in a decoded or materialized table | 10,000,000 |
| Cells in a decoded or materialized table | 640,000,000 |
| Columns in a source, output or intermediate table | 1,024 |
| Keys in one key or grouping declaration | 64 |
| Inputs to one specification | 16 |
| Operands in one source selection | 64 |
| Parent occurrences in one inheritance traversal, including aliases | 8,192 |
| Compiled program of one regular expression | 10 MiB |
| Findings in the final public issue collection | 10,000 |
| YAML nesting depth | 64 |
| Semantic expression and context nesting depth | 128 |

A MiB is 1,048,576 bytes; a GiB is 1,073,741,824 bytes. File readers check the
encoded size before retaining bytes and check growth during reading. Writers
check the saved file size before extending it. Table admission checks rows,
columns and their product before constructing or growing the corresponding
geometry. A regular expression is refused when it compiles, never while it
matches: matching takes time linear in the subject, so there is no step budget.

Semantic admission and retained observations stay complete. The final public
issue collection keeps its first 10,000 findings in their existing order. Source
identity, path authority, encoding, scalar types, handle ownership and interrupt
propagation retain their own integrity checks. These fixed limits are independent
of available process memory.

## Keyed intermediate lookups

A keyed intermediate (`intermediates: [{id, dataset, key, columns}]`) is
served by a donor index built once per build attempt over the eligible
donor rows whenever the intermediate is plain: any number of key columns
and any scalar key type (`str`, `int`, `float`, `date` or `datetime`,
including an `int` key matched against a `float` donor column or the
reverse) are indexed over the full key tuple, and donor read order is
kept inside each key bucket. Lookup shapes that still scan the eligible
donor rows for every output row are intermediates with expression keys,
a range, derivations, a correlated filter or a `SELF` source, and any
lookup over a donor relation below the small-relation index threshold.
Indexed and scanned lookups keep the same diagnostics and ordering.

## Migration

### From 0.4.0

The [0.5.0 release notes](releases/0.5.0.md#migration) list every change.
The main ones:

| Earlier spelling or behavior | Current form |
| --- | --- |
| Python `yamaa.domain(spec, environment=env)` and `yamaa.check(spec, environment=env)` | `yamaa.project("project.yaml").spec("DM").run()` and `.check()`, or `yamaa.spec(spec).run()` and `.check()` without a project. |
| Python `yamaa.define(env, "sdtm", created=...)` | `yamaa.project("project.yaml").submission("sdtm", created=...)`. |
| Python `DomainError` and `DefineError` | `yamaa.Error`, which carries `issues`. |
| `save()` returns `True` | `save()` returns nothing and raises `yamaa.Error` when it cannot write. |
| An environment file holds `language`, `codelists`, `study` and the `sdtm`, `adam` and `send` sections | A `project.yaml` holds `codelists`, `study` and the sections, and its `environments` names each language's environment file. The environment file keeps `schema_version`, `lock` and `functions`; the `environments` key states its language. |
| A section requires `standard` and `define` | A section needs only `specs`; `study`, `standard` and `define` are required only to create submission files. |
| `dataset_json: true` | `datasets: json`; `datasets: xpt` writes SAS Transport version 5 files instead. |
| `output.verification_log` and `output.warning_log`, read as `verification_log` and `warning_log` | `output.report`, read as `report`: one run report per run, with a row for the run and for each input, row template, handler, output column and evaluated check. |
| Lock pins yamaa 0.4.0 | Regenerate the project lock for 0.5.0 before using project functions; each applicable build checks the runtime version. |

The `issues` table keeps its five columns and adds `source_span` and
`operand_route` after them; `context` stays canonical JSON text.

### From 0.3.0

Make the 0.4.0 changes above as well as these:

| Earlier spelling or behavior | Current form |
| --- | --- |
| Python `yamaa_domain`, `DomainRun` and `DomainRunError` | `yamaa.spec(spec).run()` and `yamaa.Error`. |
| Python `generate_study_document` | `yamaa.project("project.yaml").submission(section)`. |
| Python subpackages such as `yamaa.odm` | `yamaa.read_odm`; the subpackages are private. |
| R `cdiscbuilder` package | The `yamaa` R package: `yamaa_spec`, `yamaa_project` and `yamaa_read_odm`. |
| A dead declaration with an invalid reference fails a file without `parents` | The declaration is removed before validation and the file builds. To keep a declaration validated, name the column in `output.columns`, `keys` or `output.order_by`, or read it from a live derivation or verification. |
| An input that no column reads is the implied driver | The input is removed too; name it in `base`. |
| `parents: []` added only to enable pruning | Every specification is pruned; remove the line. Without a `parent`, declaration order is the resolved order, so a column that reads a column declared after it fails with `forward_reference` ([COLUMN-5](../rules/columns.md#column-5)). |
| Rust code depending on `yamaa-adapters` or `yamaa-core` | Depend on the `yamaa` facade crate by path or git, as the [Rust guide](../rust/README.md#distribution) shows. Its default `yaml` feature brings the reviewed YAML parser with no other configuration; 0.4.0 itself also needs the `saphyr-parser` patch that its [release notes](releases/0.4.0.md) show. |
| `datetime_precision` of minute text (`2025-03-04T08:30`) is `S`; `datetime_impute` with `time: last` gives `08:30:00`; hour text (`T08`) is invalid | `precision` with `type: datetime` gives `minute`, `impute` with `type: datetime`, `minimum_source_precision: day` and `time: last` gives `08:30:59`, and hour text is accepted (`hour`). A condition that tested `= 'S'` for a collected time tests `IN ('minute', 'second')`; a condition meaning "a time was collected" tests `IN ('hour', 'minute', 'second')` ([TIME-7](../rules/temporal.md#time-7)). |
| Lock pins an earlier yamaa | Regenerate the project lock for the installed yamaa before using project functions; each applicable build checks the runtime version. |
| A `between.value` qualified by an input that does not drive the row reads a wrong value with no message | It fails validation as `unknown_field`. Derive the value into a column, for example `AGE` from `DM.AGE`, and name that column. |
| A `.sas7bdat` input that holds records deleted in place returns them as rows | Only its live records are read. It fails as `source_sas_invalid` with cause `deleted_records` when its count of deleted records and the records marked deleted disagree; rewrite such a dataset first, for example with `PROC COPY`. |

### From 0.2.0 and earlier

The 0.4.0 upgrade also includes language and environment changes made before
0.3.0. Update old specifications and project locks together:

| Earlier spelling or behavior | Current form |
| --- | --- |
| `implies: {when: W, then: T}` | `assert: {when: W, require: T}`; the implication semantics are preserved. |
| `assert: {expr: P}` | `assert: {require: P}`. |
| Function call with `contract_version` | Keep only `name` and `args`. |
| Environment `version`, `runtime`, per-function contracts, bindings and external conformance files | Declare `schema_version`, the project `lock`, and function definitions with `function`, `description`, `params`, `returns` and inline `tests`; the project file's `environments` key states the language. Implementations are ordinary installed packages checked against that lock. |
| Runner discovers a project environment | Name each language's environment file in the project file and open specifications through it: Python `yamaa.project("project.yaml").spec("DM")` or R `yamaa_project("project.yaml")$spec("DM")`. |
| Data roots or `yamaa-project.yaml` | Relative resources resolve from the YAML file that declares them, including inherited layers. |
| Symbolic link below the entry directory | Copy or hard-link the input; the resource policy refuses symbolic links. |

The normative contracts are [verification](../rules/verification.md),
[functions and project environments](../rules/profiles/functions.md) and
[resources](../rules/reference/files.md).
