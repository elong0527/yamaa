# Release and migration

This page records what a yamaa release supports, the behavior decisions it
keeps, and how to migrate older specifications. Release status, evidence and
open gates live in GitHub issues and pull requests, not here.

## Supported platforms

- CPython 3.12, 3.13 and 3.14 wheels and source builds on Linux x86_64,
  macOS arm64 and Windows x64. The Linux wheel is `manylinux_2_34`
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
graphs on the candidate, and CI for its minimum saved artifact readers.

Python, R and Cargo versions move together;
`tools.packaging.check_shared_release_versions` checks that they agree. The
previous Python release is the `yamaa-py-0.3.0` tag.

## Saved artifact readers

The minimum supported independent readers are:

| Reader | Minimum | Saved formats qualified |
| --- | --- | --- |
| Python Polars | 1.35.2 | CSV text and Parquet |
| Python PyArrow | 23.0.1 | CSV text and Parquet |
| R arrow | 23.0.1 | CSV text and Parquet |
| Python standard library | Supported CPython above | Dataset-JSON 1.1 |
| R jsonlite | 2.0.0 | Dataset-JSON 1.1, with `bigint_as_char = TRUE` |
| Python lxml | 6.1.0 | Define-XML 2.1 |

Python package dependencies and R package `Suggests` declare these floors.
The R readers are optional: public Build and Save use the shared native codecs.
CSV carries text rather than an embedded schema; controls supply an explicit
text schema and check exact authored CSV bytes. Its empty-field ingestion
semantics are described in the [CSV profile](../rules/storage/csv.md).
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
with the minimum versions. SAS datasets and Transport are input formats;
the published dataset profiles remain CSV and Parquet. Dataset-JSON and
Define-XML are generated through the public project submission API.

## Supported dependency combinations

The Rust workspace declares MSRV 1.90 and builds with Rust 1.90.0, the
toolchain that `rust/rust-toolchain.toml` pins. The R source package runs Cargo
inside its bundled workspace, so under rustup it builds with that toolchain and
ignores a Rust target added to another one: Windows R builds need
`rustup toolchain install 1.90.0 --target x86_64-pc-windows-gnu`. Canonical
release builds use `rust/Cargo.lock`, including unchanged copies in both source
packages. Arrow and Parquet are supported at exactly 57.3.0 with flatbuffers
25.12.19, so an embedder whose graph holds Arrow 57.3.0 builds the engine
unchanged. Other Arrow and Parquet releases are unsupported: the adapter's
version-specific APIs, confined to `storage_api.rs`, are adapted and qualified
for 57.3.0 only. Broadening that range requires separate adapter and
installed-host qualification first. The direct storage codec floors are brotli
8.0.4 and lz4_flex 0.13.1. Portable libm remains exactly 0.2.16, and the
reviewed local saphyr-parser patch remains part of every graph this workspace
builds. Cargo reads `[patch]` only from the root of the workspace being built,
so a Rust embedder adds the patch to its own workspace, as the
[Rust guide](../rust/README.md#distribution) shows; without it, the
embedder's graph takes the unpatched `saphyr-parser` from crates.io.

Python supports CPython 3.12 through 3.14 with the runtime ranges in
`python/pyproject.toml`. Their declared floors are lxml 6.1.0, Polars 1.35.2,
py-yaml12 0.2, PyArrow 23.0.1, Pydantic 2.12.4, packaging 25 and PyYAML 6.0.3.
The Parquet 57.3.0 writer records only type-defined column orders, which
Polars 1.35.2 reads; the IEEE 754 total-order form that later writers record
for floating-point columns needs Polars 1.43.2. R uses R 4.6.1 and the shared
Rust graph; its host qualification readers are the `Suggests` dependencies in
`R/yamaa/DESCRIPTION`.

The [dependency graph lanes](../CONTRIBUTING.md#ci-and-release-qualification)
test declared direct minima and newest compatible selections on Linux with
CPython 3.12 and 3.14 and R 4.6.1. Cargo transitive dependencies use the newest
MSRV-compatible resolution in both lanes; local patches and exact pins are
preserved. Python runtime/test floors are exact in the minimum lane, while
Python transitives are resolved compatibly. These isolated graphs do not change
`rust/Cargo.lock` or `python/uv.lock`. The full native matrix separately qualifies
the locked release across supported platforms and Python versions.

## Publication

After the final commit is on main and its complete native qualification has
passed, push the coordinated tag `yamaa-v0.4.0` at that exact commit.
`.github/workflows/release.yml` accepts a completed scheduled main run, a main
dispatch, or a same-repository `qualification/issue-*` push for the peeled tag
commit. It requires all installed lanes, shared Rust/math, complete cohort
comparisons and strict R package checks to have passed. Branch dispatches cannot
release.

The workflow downloads the qualified artifacts and publishes their unchanged
bytes: one wheel per supported OS, the tested Linux Python sdist and the tested
Linux R source tarball. Other native source lanes qualify their own host-built
archives. Missing, expired or ambiguous artifacts fail publication; an existing
release is never overwritten. The release includes committed notes from
`docs/releases/<version>.md` and identifies the native run and selected artifacts.
This route uses the workflow's GitHub token and needs no package-index
credentials. The workflow has no PyPI step: 0.4.0 is distributed as GitHub
release assets only, and those assets do not establish PyPI publication. PyPI
publication is a separate decision and requires configured authentication;
the GitHub release assets remain the documented standalone download route.

## Source package contents

Python and R source packages carry the same Rust workspace, static ReadStat
bridge and patched YAML parser. SAS and Transport decoding runs in the shared
adapter over captured source bytes; it needs no host reader fallback or dynamic
ReadStat installation. The [ReadStat integration](../rust/crates/yamaa-adapters/vendor/readstat/README.md)
and [YAML parser provenance](../rust/vendor/saphyr-parser/PROVENANCE.md)
own their source inventories, patches and licenses. Both parser authors' MIT
notices remain in the distributed `LICENSE-YAML.txt`.

Every package carries the project license and the third-party notices: each
wheel in its `.dist-info/licenses/` directory, the Python source archive at its
root, and the R source package in `inst/`, which R installs at the package
root. `LICENSE.txt` is the project's MIT license. `LICENSE-THIRD-PARTY.txt`
reproduces the license, copyright and NOTICE texts of every third-party crate
in the normal and build dependency closure of the shipped Python and R
binding crates, for every platform, and names the license relied on where a
crate offers a choice. `LICENSE-READSTAT.txt`,
`LICENSE-UNICODE.txt` and `LICENSE-YAML.txt` cover the vendored ReadStat
sources, the Unicode data in yamaa's own tables and the vendored YAML parser.
The R package keeps its `MIT + file LICENSE` declaration, and its `Copyright`
field names these files. The Python License-Expression,
`MIT AND Apache-2.0 AND BSD-3-Clause AND Unicode-3.0`, covers the project and
the crates linked into the wheels.

`tools.packaging.generate_third_party_notices` generates the crate notices
from `rust/Cargo.lock` and derives that expression. The repository check fails
when the locked graph gains or loses a crate without a matching notice entry.
Native qualification and the release selector require every wheel and source
package to carry each notice unchanged.

Native qualification exercises the static source bridge on every supported
platform and compares both installed hosts with independently authored binary
fixtures and exact saved bytes. The [SAS source contract](../rules/storage/sas.md)
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
  original failures, callback counts and producer-first publication order.
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
  declares no `parents`; resolved-column ordering stays inheritance-only.
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
- Refuse a SAS7BDAT dataset that holds records deleted in place as
  `source_sas_invalid` with cause `deleted_records`. Reading only its live
  records is 0.5.0 work (#2924).
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
Admission checks the physical file, table geometry, declaration shape or actual
regex match at its owning boundary. There are no caller-selected profiles or
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
| Steps in one regex match | 1,000,000 |
| Findings in the final public issue collection | 10,000 |
| YAML nesting depth | 64 |
| Semantic expression and context nesting depth | 128 |

A MiB is 1,048,576 bytes; a GiB is 1,073,741,824 bytes. File readers check the
encoded size before retaining bytes and check growth during reading. Writers
check the saved file size before extending it. Table admission checks rows,
columns and their product before constructing or growing the corresponding
geometry. The regex step counter is fresh for each match.

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

### From 0.3.0

| Earlier spelling or behavior | Current form |
| --- | --- |
| Python `yamaa.domain(spec, environment=env)` and `yamaa.check(spec, environment=env)` | `yamaa.project("project.yaml").spec("DM").run()` and `.check()`, or `yamaa.spec(spec).run()` and `.check()` without a project. |
| Python `yamaa.define(env, "sdtm", created=...)` | `yamaa.project("project.yaml").submission("sdtm", created=...)`. |
| Python `yamaa_domain`, `DomainRun` and `DomainRunError` | `yamaa.spec(spec).run()` and `yamaa.Error`. |
| Python `generate_study_document` | `yamaa.project("project.yaml").submission(section)`. |
| Python `DomainError` and `DefineError` | `yamaa.Error`, which carries `issues`. |
| `save()` returns `True` | `save()` returns nothing and raises `yamaa.Error` when it cannot write. |
| Python subpackages such as `yamaa.odm` | `yamaa.read_odm`; the subpackages are private. |
| R `cdiscbuilder` package | The `yamaa` R package: `yamaa_spec`, `yamaa_project` and `yamaa_read_odm`. |
| An environment file holds `language`, `codelists`, `study` and the `sdtm`, `adam` and `send` sections | A `project.yaml` holds `codelists`, `study` and the sections, and its `environments` names each language's environment file. The environment file keeps `schema_version`, `lock` and `functions`; the `environments` key states its language. |
| A section requires `standard` and `define` | A section needs only `specs`; `study`, `standard` and `define` are required only to create submission files. |
| `dataset_json: true` | `datasets: json`. |
| `output.verification_log` and `output.warning_log`, read as `verification_log` and `warning_log` | `output.report`, read as `report`: one run report per run, with a row for the run and for each input, row template, handler, output column and evaluated check. |
| A dead declaration with an invalid reference fails a file without `parents` | The declaration is removed before validation and the file builds. To keep a declaration validated, name the column in `output.columns`, `keys` or `output.order_by`, or read it from a live derivation or verification. |
| An input that no column reads is the implied driver | The input is removed too; name it in `base`. |
| `parents: []` added only to enable pruning | Every specification is pruned, so the line no longer affects pruning. Any `parents` field, even an empty one, still orders resolved columns by dependency; without one, declaration order is the resolved order ([REQ-0060](../rules/execution/lifecycle.md#req-0060), [REQ-0643](../rules/specification/composition.md#req-0643)). Remove the line only once no column reads a column declared after it; otherwise the specification fails with `forward_reference` ([REQ-0071](../rules/execution/lifecycle.md#req-0071)). |
| Rust code depending on `yamaa-adapters` or `yamaa-core` | Depend on the `yamaa` facade crate by path or git and, with its default `yaml` feature, add the `saphyr-parser` patch that the [Rust guide](../rust/README.md#distribution) shows. |
| `datetime_precision` of minute text (`2025-03-04T08:30`) is `S`; `datetime_impute` with `time: last` gives `08:30:00`; hour text (`T08`) is invalid | `M`, `08:30:59`, and hour text is accepted (`H`). `IN ('M', 'S')` selects what `= 'S'` selected in 0.3.0, and only those forms convert directly to a `datetime`. A condition meaning "a time was collected" tests `IN ('H', 'M', 'S')`, because hour text also carries a collected time ([REQ-1327](../rules/operations/temporal.md#req-1327)). Complete hour text with `datetime_impute`; it is not a `datetime` ([REQ-1329](../rules/operations/temporal.md#req-1329)). |
| Lock pins an earlier yamaa | Regenerate the project lock for 0.4.0 before using project functions; each applicable build checks the runtime version. |
| A `between.value` qualified by an input that does not drive the row reads a wrong value with no message | It fails validation as `unknown_field`. Derive the value into a column, for example `AGE` from `DM.AGE`, and name that column. |
| A `.sas7bdat` input that holds records deleted in place returns them as rows | It fails as `source_sas_invalid` with cause `deleted_records`. Rewrite the dataset first, for example with `PROC COPY`. |

The `issues` table keeps its five columns and adds `source_span` and
`operand_route` after them; `context` stays canonical JSON text.

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

The normative contracts are [verification](../rules/execution/verification.md),
[functions and project environments](../rules/operations/functions.md) and
[resources](../rules/storage/resources.md).
