# Release and migration

This page records what a yamaa release supports, the behavior decisions it
keeps, and how to migrate older specifications. Release status, evidence and
open gates live in GitHub issues and pull requests, not here.

## Supported platforms

- CPython 3.12, 3.13 and 3.14 wheels and source builds on Linux x86_64,
  macOS arm64 and Windows x64.
- R 4.6.1 source builds on the same three hosts, each passing strict
  `R CMD check` with **Status: OK**.
- Other runtimes and architectures, free-threaded Python, musl and R binary
  packages are not supported. R is distributed as a GitHub source tarball;
  CRAN and r-universe are later work.

`.github/workflows/native.yml` builds, installs and tests every supported
combination nightly on main and on manual dispatch. A release is cut from a
commit whose complete native run passed.

Python, R and Cargo versions move together;
`tools.packaging.check_shared_release_versions` checks that they agree. The
previous Python release is the `yamaa-py-0.3.0` tag.

## Publication

After the final commit is on main and its complete native qualification has
passed, push the coordinated tag `yamaa-v0.4.0` at that exact commit.
`.github/workflows/release.yml` accepts a completed nightly main run, a main
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
credentials. PyPI publication is separate and requires configured authentication;
the GitHub release assets remain the documented standalone download route.

## Source package contents

Python and R source packages carry the same Rust workspace, static ReadStat
bridge and patched YAML parser. SAS and Transport decoding runs in the shared
adapter over captured source bytes; it needs no host reader fallback or dynamic
ReadStat installation. The [ReadStat integration](https://github.com/elong0527/yamaa/blob/main/rust/crates/yamaa-adapters/vendor/readstat/README.md)
and [YAML parser provenance](https://github.com/elong0527/yamaa/blob/main/rust/vendor/saphyr-parser/PROVENANCE.md)
own their source inventories, patches and licenses. Both parser authors' MIT
notices remain in the distributed `LICENSE-YAML.txt`.

Native qualification exercises the static source bridge on every supported
platform and compares both installed hosts with independently authored binary
fixtures and exact saved bytes. The [SAS source contract](https://github.com/elong0527/yamaa/blob/main/rules/storage/sas.md)
owns decoding behavior.

## Approved decisions

- Keep Python `domain`/`check`/`define`, result/error types, version metadata and
  `read_odm`/`write_odm_parquet`; keep the R equivalents, including
  `yamaa_define`. Legacy evaluators, probes, resolved-spec/IO/authoring
  extensions and `cdiscbuilder` are retired. The style CLI remains private and
  native-backed.
- Keep Python `environment` keyword-only, wrong argument types as `TypeError`,
  R argument failures as `yamaa_argument_error`, and missing specifications as
  issue rows. Check returns issues, without a public resolved-spec producer.
- Preserve full signed-i64 values and separate missingness, lossless R
  `yamaa_int64_vector`/`yamaa_utf8_vector`, preserve-or-error conversions and
  JSON-text diagnostic context.
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
  [portable_math.tsv](https://github.com/elong0527/yamaa/blob/main/rust/crates/yamaa-core/tests/fixtures/portable_math.tsv).
- Keep canonical Cargo locking and unchanged source-package copies. Runtime,
  contract, cache and resource identities use references, canonical forms or
  bytes; only packaging-tool lockfiles carry their own checksum fields.

Host behavior and source installation commands live in the
[Python](https://github.com/elong0527/yamaa/blob/main/python/README.md) and
[R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) guides.

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

The shared Rust engine behind Python and R `domain`, `check` and `define` calls
uses one fixed policy in `yamaa_core::limits::Limits`.
Admission checks the physical file, table geometry, declaration shape or actual
regex match at its owning boundary. There are no caller-selected profiles or
cumulative byte, copy, traversal or work pools.

The standalone Python `read_odm` and `write_odm_parquet` helpers retain their
own archive-member, expanded-byte and XML-depth options and report parser
refusals through `ODMError`.

| Boundary | Fixed maximum |
| --- | ---: |
| Each specification, schema, environment, function, codelist or packaging-lock file | 8 MiB |
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

## Specification and environment migration from 0.2.0

The 0.4.0 upgrade also includes language and environment changes introduced
before 0.3.0. Update old specifications and project locks together:

| Earlier spelling or behavior | Current form |
| --- | --- |
| `implies: {when: W, then: T}` | `assert: {when: W, require: T}`; the implication semantics are preserved. |
| `assert: {expr: P}` | `assert: {require: P}`. |
| Function call with `contract_version` | Keep only `name` and `args`. |
| Environment `version`, `runtime`, per-function contracts, bindings and external conformance files | Declare `schema_version`, `language`, the project `lock`, and function definitions with `function`, `description`, `params`, `returns` and inline `tests`. Implementations are ordinary installed packages checked against that lock. |
| Runner discovers a project environment | Pass it explicitly: Python `domain(spec, environment="python/environment.yaml")` or R `yamaa_domain(spec, environment="r/environment.yaml")`. |
| Data roots or `yamaa-project.yaml` | Relative resources resolve from the YAML file that declares them, including inherited layers. |
| Symbolic link below the entry directory | Copy or hard-link the input; the resource policy refuses symbolic links. |
| Lock pins an earlier yamaa | Regenerate the project lock for 0.4.0 before using project functions; each applicable build checks the runtime version. |

The normative contracts are [verification](https://github.com/elong0527/yamaa/blob/main/rules/execution/verification.md),
[functions and project environments](https://github.com/elong0527/yamaa/blob/main/rules/operations/functions.md) and
[resources](https://github.com/elong0527/yamaa/blob/main/rules/storage/resources.md).
