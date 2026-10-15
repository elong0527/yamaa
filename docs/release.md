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
- Resolve resources from their original declaring file, with absolute and
  parent-relative local paths and no project-root fallback. Preserve snapshots,
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

Capacity refusals retain `engine_rejected`, a null requirement and the authored
paths. Compiler refusals identify `code: compiler_limit`, `stage: prepare`, the
resource, its integer limit and actual usage when counted. Numeric policy
refusals use `code: numeric_policy`; preparation and YAML resource refusals use
`code: resource_limit` and retain their phase. Check and Build return the same
preparation facts, with or without a project environment.

Typed resource refusals also identify `incomplete_checks_from`, the earliest
lifecycle scope without complete check results: `specification` for capture and
preparation, `source_binding` for input admission and binding, `data` for
execution and verification, or `output` for serialization and publication.
Build/report boundaries use `build` when no narrower scope is available; other
boundaries retain their stage name. Earlier completed checks remain valid. A
refusal does not enumerate which individual requirements ran, execute later
checks, or make static Check read study data. Reduce the exhausted resource and
rerun to obtain remaining findings.

## Resource limits

Public Python and R calls use `ExecutionBudget::longitudinal()`: 10,000,000
source/output rows, 640,000,000 cells, 1 GiB scalar text, 512 MiB output text,
and 1 GiB encoded input/output. Dataset execution has an independent
2,000,000,000 logical-work allowance. CSV defaults admit the same source
profile, including up to 1,024 header fields and the header record. The
producer frontier shares these allowances. Smaller caller-selected budgets and
the `ExecutionBudget::bounded()` profile remain enforced. Lower decoder, CSV,
IPC, report and publication ceilings must not refuse an otherwise admitted
input below this profile.

Integer literals must fit signed i64 and otherwise report `integer_overflow`
(REQ-0014). CSV `output.decimals` accepts 0 through 18. Resource bounds remain
separate from schema validity and mathematical function behavior.

Specifications admit 1,024 declared columns, output columns, typed input
fields, CSV/Parquet/Arrow source fields and producer contract fields; 1,025 is
refused at its corresponding boundary. Producer declarations also allow 1,024,
because each selected contract field requires one declaration. Keys remain
limited to 64, input datasets to 16 and source operands to 64. An `ordinal`
field counts toward the source width.

Validation and normalization start with 67,108,864 logical work units; the
separate schema-bundle work ceiling remains 4,194,304. Specification
preparation adds one unit per original decoded node and text byte to that flat
allowance. Inheritance uses four times the flat allowance for the same total
input size: one share each for layer admission, composition/dependencies, final
normalization and model admission. Thus `W_flat(N, B) = 67,108,864 + N + B` and
`W_layered(N, B) = 4 * W_flat(N, B)`, with `N` and `B` summed over unique
captured layers, including the entry. Input nodes and text bytes keep their
separate 131,072 and 8,388,608 limits, so layered work stops at at most
302,514,176 units. All stages and failed attempts share cumulative charges;
branches and repeated parents cannot reset them. Custom Rust work allowances
keep a finite size credit of at most their original work allowance.

Flat preparation permits `65,536 + 2 * N` attempted diagnostics and
`8,388,608 + 128 * N + B` bytes of diagnostic text. These counters include
failed union alternatives that are discarded after another alternative
succeeds; emitted findings and their ordering remain unchanged. Layered
preparation uses four times those same allowances, capped at 1,310,720 attempts
and 134,217,728 text bytes. Cumulative normalized arena construction has eight
shares for descriptor output/import, composition and the subsequent planning
arenas: 1,048,576 nodes, 67,108,864 text bytes and 2,097,152 edges. Each
individual decoded, normalized and composed document keeps its 131,072-node,
8,388,608-text-byte and 262,144-edge bounds. Traversal refuses more than 8,192
parent occurrences, including repeated occurrences of the same parent. Its
copied path bytes keep their 1,048,576-byte cumulative ceiling. Schema regex
matching keeps the individual-match limits while its cumulative subject-byte,
work and state-cell allowances receive the same four shares plus four more for
dependency reachability/ordering, and a size credit capped at each original
allowance. Their maximums are 16,777,216 subject bytes, 16,000,000 work units and
16,000,000 state cells; runtime expression matching keeps its separate policies.
These size allowances belong to top-level host preparation; explicit Rust
component budgets created with `NormalizationBudget::new` keep their exact
caller-selected ceilings and cumulative accounting.

Representation allowances derive from the admitted geometry. Let `C` be
640,000,000 cells, `R` be 10,000,000 rows, `S` be 1 GiB source/scalar text,
`T` be 512 MiB output text, and `H` be 64 MiB metadata slack:

- Source storage/decode allowance: `32*C + 24*R + 3*S + H`, covering
  physical/canonical slots, row owners and text conversion/ownership.
  SAS, CSV and Parquet use this central allowance and enforce smaller selected
  quotas. Compressed SAS/Parquet text can exceed encoded input size; its actual
  modeled decoded storage/work must still fit this finite allowance.
- Internal public Arrow/Option/IPC transport: `64*C + T + H`. Authored artifacts
  keep their separate 1 GiB encoded-output limit.
- Descriptor capture/reread allowance: `4*S`; copied original-source evidence:
  `S + H`. Each capture still enforces the caller's encoded-byte maximum.
- Parquet source groups admit the longitudinal 65,536-batch allowance. Compact
  metadata/header bytes, nodes and schema tables remain bounded by the encoded
  source allowance; pages admit up to `C`. These allow the writer's 1,024-row
  groups to round-trip without a lower group/footer ceiling.
- Report traversal cells: `P = C + 3*C`, covering source/derived/artifact visits
  and the three public typed-copy passes.
- Complete report encoded bytes: `E = 256*P + 12*(S + T + S) + 16*(R + R) + H`.
  Fixed cell slots cover typed maps/scalar spellings, and text factors cover
  JSON escaping and artifact records.
- Report retained charges: `6*E`; report work: `8*E + 16*P`. Entries, buffer
  writes and logical cell visits keep their actual independent charges.

These are logical admission policies; they do not reserve memory or guarantee
that the host has enough process memory.

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
