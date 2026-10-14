# Release and migration

Current release checklist:
[#2288](https://github.com/elong0527/yamaa/issues/2288), with the maintainer's
2026-10-14 capacity and performance decisions. It supersedes #1969, #1973,
#1994, #1998, #1999, #2142, #2144, #2145 and #2147. The harness and auditor
owners #2141 and #2143 are completed. The remaining assertion migration and
cleanup belong to [#2289](https://github.com/elong0527/yamaa/issues/2289).
Historical qualification observations below keep their original tested revisions.

The [final release handoff](release-handoff.md) reconciles every entry gate,
independently re-admitted rollback receipts, measured-source renewal and exact
package selection. Transition acceptance and the final package selection remain
pending there until all evidence gates pass.

## State

Compact flow verifications such as
`verifications: [row_count: {min: 3, max: 3}]` use a local crates.io patch of
saphyr-parser 0.2.1. Collection-local scanner state preserves nested mappings,
independent of preceding flow mappings; explicit and block spellings retain the
same behavior. The vendored release sources, narrow patch and original licenses
are documented in `rust/vendor/saphyr-parser/PROVENANCE.md`. Both native source
package routes retain and verify those bytes. Installed Python/R controls cover
equal output and saved CSV bytes, both bounds and malformed closers. This repair
does not qualify the complete release matrix described below.

| State | Observed scope |
| --- | --- |
| Implemented and merged | Shared language execution; Python/R domain/check/define; runner conversion; legacy evaluator/API/probe and R/cdiscbuilder removal; coordinated Python/R/Cargo 0.4.0 source versions. The performance harness is merged through PR #2137; subsequent native dependency/guard, portable-math control and performance admission repairs are merged through PRs #2164-#2168. |
| Qualified at earlier revisions | Retained public-conformance ratchet: run 38416920074, attempt 1, source `2ba10418607933f92597f7c00ac363a3113a49db`; earlier rollback rehearsals below. Their results do not qualify current main or its 0.4.0 composition. |
| Pending evidence | Final combined-main installed/numerical audit, implementation and installed proof of the approved longitudinal capacity, the Linux performance measurements below, remaining assertion migration under #2289, and exact artifact handoff. |
| Published | [Python source release 0.3.0](https://github.com/elong0527/yamaa/releases/tag/yamaa-py-0.3.0), source `845a91ff`, published 2026-10-11, with no package assets. No shared 0.4.0 release is listed. Prior source-built Python rollback packages and the pre-cutover R 0.3.0 candidate remain unpublished drafts. |

PR [#2165](https://github.com/elong0527/yamaa/pull/2165) merged without a passing
exact-head installed matrix. Its main run
[38503400832](https://github.com/elong0527/yamaa/actions/runs/38503400832) began
after merge, and qualification run
[38503459253](https://github.com/elong0527/yamaa/actions/runs/38503459253) was
canceled. Neither satisfies the required pre-merge gate; no approved exception
superseded that requirement. Final combined-main acceptance remains pending.

Removal and version changes landed **before** final evidence-based item-10/item-11
acceptance. Planning removal in PR #2161 and math-record replacement in PR #2162
also preceded this consolidated guide. Record the actual chronology; completed
implementation, closed work packages and approved target policy do not imply
transition approval. #2288 now owns the remaining release evidence.
Publication/tagging/asset
replacement is a separate operation.

## Approved decisions

[#1973](https://github.com/elong0527/yamaa/issues/1973) records items 1-9 and 12-19
as decided. The [approval comment](https://github.com/elong0527/yamaa/issues/1973#issuecomment-6112883568)
and [original decision source](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/maintainer-decisions.md)
retain provenance. Their current policy is:

- Retain Python `domain`/`check`/`define`, result/error types, version metadata and
  `read_odm`/`write_odm_parquet`; retain R equivalents including `yamaa_define`.
  Legacy evaluators, probes, resolved-spec/IO/authoring extensions and
  `cdiscbuilder` are retired. The style CLI remains private and native-backed.
- Keep Python `environment` keyword-only, wrong argument types as `TypeError`,
  R argument failures as `yamaa_argument_error`, and missing specifications as
  issue rows. Check returns issues, without a public resolved-spec producer.
- Preserve full signed-i64 values and separate missingness, lossless R
  `yamaa_int64_vector`/`yamaa_utf8_vector`, preserve-or-error conversions and
  JSON-text diagnostic context. No JSON callback request/reply protocol remains.
- Resolve resources from their original declaring file, with approved absolute
  and parent-relative local paths and no project-root fallback. Preserve snapshots,
  original failures, callback counts and producer-first publication order.
- Keep normative diagnostics registry-owned, including the reviewed
  `unsupported_operation` and `engine_rejected` null-requirement exceptions.
  Cut breaks remain finite/strictly ascending; signed-i64 predicate overflow,
  CSV empty-string refusal, greatest/least first-source ties and row-filter
  behavior follow the normative rules and independent expected truth.
- Use PortableLibmV1 and pinned libm 0.2.16. EXP/LN/POWER results, downstream
  predicates, conversions and saved bytes may differ from historical platform
  math. This promises neither universal correct rounding nor historical bit
  compatibility. Current independent goldens live in
  [portable_math.tsv](https://github.com/elong0527/yamaa/blob/main/rust/crates/yamaa-core/tests/fixtures/portable_math.tsv);
  final qualification still requires all nine assessments and exact comparison.
  The [original numerical decision](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/1740-math-decision.md)
  and [witness report](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/qualification/math-decision-v1.json)
  remain in Git. Replaying the unchanged archived certificate algorithm against
  that self-contained report reproduces all 2,304 original certificate objects
  (2,246 union and 58 boundary cases), including exclusions and mathematically
  different results. The 26 unchanged archived controls also pass. This bounded
  historical replay establishes neither universal correct rounding nor current
  installed qualification; the latter requires the complete final-source matrix.
- Qualify CPython 3.12/3.13/3.14 wheel and independent source builds on Linux
  x86_64, macOS arm64 and Windows x64 (18 tuples), plus R 4.6.1 source builds on
  the same hosts (three tuples), each with strict `R CMD check` **Status: OK**.
  Other runtimes/architectures, free-threaded Python, musl and R binary packages
  are outside this first release. R distribution starts with a staged GitHub
  source tarball; CRAN/r-universe are later work.
- Keep canonical Cargo locking and unchanged source-package copies. Runtime,
  contract, cache and resource identities use references/canonical forms/bytes;
  only packaging-tool lockfiles carry their own checksum fields.

The current #2288 decision adopts the public hosts' longitudinal profile:
10,000,000 source/output rows, 640,000,000 cells, 1 GiB scalar text,
512 MiB output text and 1 GiB output. Lower decoder, CSV, IPC, report and
publication ceilings must not refuse an otherwise admitted input below this
profile. Implementation and the paired 100-column, 50,000-record Build/Save
control remain release evidence obligations. Preparation work for inherited
documents is tracked separately by #2300.

These are target decisions, not qualification results. Host behavior and source
installation commands have one home in the
[Python](https://github.com/elong0527/yamaa/blob/main/python/README.md) and
[R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) guides.

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
[resources](https://github.com/elong0527/yamaa/blob/main/rules/storage/resources.md). The original changes are
[#1782](https://github.com/elong0527/yamaa/pull/1782) and
[#1856](https://github.com/elong0527/yamaa/pull/1856); retired requirement
provenance remains in Git history.

Integer literals must fit signed i64 and otherwise report `integer_overflow`
(REQ-0014). CSV `output.decimals` accepts 0 through 18. The approved width is
1,024 columns; the new longitudinal limits are recorded above. Resource bounds
remain separate from schema validity and mathematical function behavior.

The width decision in [#2299](https://github.com/elong0527/yamaa/issues/2299),
implemented in [#2327](https://github.com/elong0527/yamaa/pull/2327), admits
1,024 declared columns, output columns, typed input fields, CSV/Parquet/Arrow
source fields and producer contract fields; 1,025 is refused at its corresponding
boundary. Producer declarations also allow 1,024, because each selected contract
field requires one declaration. Keys remain limited to 64, input datasets to 16
and source operands to 64. An `ordinal` field counts toward the source width.
Validation and normalization start with 67,108,864 logical work units; the
separate schema-bundle work ceiling remains 4,194,304. Specification preparation
adds one unit per original decoded node and text byte to that flat allowance.
The [#2300](https://github.com/elong0527/yamaa/issues/2300) inheritance policy uses
four times the flat allowance for the same total input size: one share each for
layer admission, composition/dependencies, final normalization and model
admission. Thus `W_flat(N, B) = 67,108,864 + N + B` and
`W_layered(N, B) = 4 * W_flat(N, B)`, with `N` and `B` summed over unique captured
layers, including the entry. Input nodes and text bytes retain their separate
131,072 and 8,388,608 limits, so layered work stops at at most 302,514,176 units.
All stages and failed attempts share cumulative charges; branches and repeated
parents cannot reset them. Custom Rust work allowances keep a finite size credit
of at most their original work allowance.

Flat preparation permits `65,536 + 2 * N` attempted diagnostics and
`8,388,608 + 128 * N + B` bytes of diagnostic text. These counters include failed
union alternatives that are discarded after another alternative succeeds;
emitted findings and their ordering remain unchanged. Layered preparation uses
four times those same allowances, capped at 1,310,720 attempts and 134,217,728
text bytes. Cumulative normalized arena construction has eight shares for
descriptor output/import, composition and the subsequent planning arenas:
1,048,576 nodes, 67,108,864 text bytes and 2,097,152 edges. Each individual decoded,
normalized and composed document keeps its original 131,072-node,
8,388,608-text-byte and 262,144-edge bounds. Traversal still refuses more than
8,192 parent occurrences, including repeated occurrences of the same parent.
Its copied path bytes also retain their 1,048,576-byte cumulative ceiling.
Schema regex matching retains the existing individual-match limits while its
cumulative subject-byte, work and state-cell allowances receive the same four
shares plus four more for dependency reachability/ordering, and a size credit
capped at each original allowance. Their maximums are
16,777,216 subject bytes, 16,000,000 work units and 16,000,000 state cells; runtime
expression matching keeps its separate existing policies.
These size allowances belong to top-level host preparation; explicit Rust
component budgets created with `NormalizationBudget::new` retain their exact
caller-selected ceilings and cumulative accounting.
Installed Python/R controls cover
flat, explicit empty-parent, empty-child and two-layer case specifications at
40, 64 and 1,024 columns, including independent expected output and saved bytes.
The earlier source/output cell ceilings of 2,097,152 and 1,048,576 are
superseded by the approved longitudinal defaults above. This width policy does
not qualify the broader capacity or installed
release gates under [#2288](https://github.com/elong0527/yamaa/issues/2288).

Public Python and R calls now use `ExecutionBudget::longitudinal()`: 10,000,000
source/output rows, 640,000,000 cells, 1 GiB scalar text, 512 MiB output text,
and 1 GiB encoded input/output. Dataset execution has an independent
2,000,000,000 logical-work allowance. CSV defaults admit the same source profile,
including up to 1,024 header fields and the header record. The producer frontier
shares these allowances. Smaller caller-selected budgets and the original
`ExecutionBudget::bounded()` profile remain enforced.

Representation allowances derive from the admitted geometry. Let `C` be
640,000,000 cells, `R` be 10,000,000 rows, `S` be 1 GiB source/scalar text,
`T` be 512 MiB output text, and `H` be 64 MiB metadata slack:

- Source storage/decode allowance: `32*C + 2*S + H`, covering physical/canonical
  slots, text payload and encoded/header storage.
- Internal public Arrow/Option/IPC transport: `64*C + T + H`. Authored artifacts
  retain their separate 1 GiB encoded-output limit.
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
  JSON escaping and retained artifact records.
- Report retained charges: `6*E`; report work: `8*E + 16*P`. Entries, buffer
  writes and logical cell visits keep their actual independent charges.

These are finite logical admission policies; they do not reserve memory or
guarantee that the host has enough process memory.

Paired installed-host controls build and save a 100-column, 50,000-row CSV
source and compare all saved bytes to independent direct-projection truth. Both
hosts also build a consumer through a Parquet-producing dependency. Python
checks a CSV larger than 64 MiB, independent Parquet input and Parquet
publication's complete values. Explicitly selected `ExecutionBudget::bounded()`
and caller-provided smaller quotas remain enforced. Complete installed
release qualification remains under
[#2288](https://github.com/elong0527/yamaa/issues/2288).

## Remaining gates and owners

The historical B01-B19 identifiers remain useful for reconciliation. Closed
implementation owners are not reopened; the following owners renew final evidence.

| Gate | Current disposition and acceptance owner |
| --- | --- |
| B01 - active conformance | Current directory-derived active corpus, with two explicit historical exclusions; final 21-tuple admission: #2288. |
| B02 - Python API | Shared facade merged; final signatures/results/arguments/helpers and export/archive reconciliation: #2288. |
| B03 - R API/scalars | Approved i64/NUL/error/Define surface merged; renew installed ownership/conversions: #2288. |
| B04 - filesystem/platforms | Declaring-file paths and Windows controls merged; renew final installed source: #2288. |
| B05 - probe/JSON retirement | Removal merged; reconcile unique retired assertions and retained consumers: #2289. |
| B06 - route/environment selection | Unified composition merged; final full installed evidence: #2288. |
| B07 - producers | Orchestration merged; final port/callback/failure/publication order: #2288. |
| B08 - submission | Shared Define and registry ownership merged; final installed render/Save/default-version controls: #2288. |
| B09 - language | Language families merged; renew complete active conformance and predicate/intermediate contracts: #2288. |
| B10 - codecs/publication | Implemented; renew exact CSV/logical Parquet, original failures and held Saves: #2288. |
| B11 - diagnostics | Shared ownership and null exceptions decided; final registry/installed controls: #2288. |
| B12 - numerical | PortableLibmV1 decided; final public truth, nine assessments and exact comparison: #2288. |
| B13 - builds/locks | Canonical locks and 0.4.0 versions merged; renew actual archive membership, byte equality, features/toolchains: #2288. |
| B14 - content identity | Policy resolved by the lockfile exception above; build evidence belongs to B13. |
| B15 - independent audit | Audit implemented; passing post-removal combined-main verdict and original-input offline replay: #2288. |
| B16 - lifecycle/removal | Installed controls retained; final ownership/interruptions/reentrancy/repeated use/Save: #2288; removal reconciliation: #2289. |
| B17 - performance/capacity | Harness complete; approved capacity implementation and scoped Linux measurements remain under #2288. |
| B18 - numeric budgets | The maintainer sets no performance pass/fail budgets for 0.4.0. Retain observed latency/RSS and check coverage with the independent auditor. |
| B19 - transition/rollback | Rollback receipts already re-admitted; exact final tested package selection and artifact handoff: #2288. |

The remaining native release qualification still covers all 21 installed tuples.
Performance for 0.4.0 is a smaller scope: all 27 authored cases (nine dimension
variants at 1k/10k/100k) on Linux, using an installed Python 3.12 wheel and an
R 4.6.1 source installation. Keep ten cold samples, three retained warmups and
thirty warm samples per case. Use `collect_public_performance.py`, independently
audit that declared scope with `audit_public_performance.py`, and record observed
latency and RSS here once the measured run completes. No numeric pass/fail
budgets are set. The original 21-tuple campaign remains historical tooling;
its acceptance requirements are not the current release decision.

Independent typed/output truth, original clocks, RSS, lifecycle observations and
both Saves remain required. Measure without concurrent compilation. Logical
rows/cells/work, captured bytes, graph ownership, encoded reports and publication
prefixes have separate bounded scopes. Admission counters do not cap whole-process
RSS. Refusals and unavailable/incomparable metrics retain explicit dispositions.

## Qualification and retained records

Use [CI guidance](https://github.com/elong0527/yamaa/blob/main/.github/ci.md)
for sampled PR coverage, the full native route and independent audit/replay.
A branch dispatch supplies the installed matrix and component numeric checks;
shared Rust, nine math assessments/comparison and audit run on the nightly main
run, a dispatch on main or an explicit qualification push. Main's audit is
nonblocking: inspect its verdict and replay the original inputs. Branch
qualification stays `reviewed-branch`; only genuine postmerge evidence is
`combined-main`.

Benchmark directories and the public-conformance configuration determine the
current authored inventory and explicit exclusions. The generated
[benchmark index](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#benchmarks)
accounts for the current authored cases, the active cases shared by Python
and R and the two reviewed path-history exclusions. Runnable host entries describe
metadata. Each installed inventory must supply actual passing results for the
same current active set; the earlier JSON observations keep their original
source/run/attempt/artifact identities and do not qualify a later source.

New native audit snapshots retain exact-source benchmark specification paths in
`source.json` and the exclusion configuration in
`source/rust/tools/public_conformance.py`. Offline replay of a pre-change retained
snapshot uses its original `source/benchmarks/execution-manifest.yaml` and metadata.
This retained-input reader does not restore a runtime API or infer qualification
from current checkout labels. Original retained-snapshot replay and final
installed Python/R results remain evidence gates under #2288.

The live inventory is:

| Record | Reader and purpose |
| --- | --- |
| [Current rules](articles/rules.md) and [schema-navigation.tsv](https://github.com/elong0527/yamaa/blob/main/.github/scripts/yaml-validation/fixtures/schema-navigation.tsv) | `check_rule_rewrite.py` derives current normative ownership; `test_rule_rewrite.py` checks the independent schema-owner fixture. `generate_rule_reference.py` derives navigation from current contracts and schemas. |
| [release-api.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/release-api.json) | `check_release_api.py` and its tests compare declared exports/dispositions. `retain_candidate` labels remain inventory classifications, not final acceptance. |
| [benchmarks/](https://github.com/elong0527/yamaa/tree/main/benchmarks) and [public_conformance.py](https://github.com/elong0527/yamaa/blob/main/rust/tools/public_conformance.py) | Directories and explicit exclusions derive the current inventory, index/dashboard membership and the same mandatory active set for both installed hosts. |
| [public-conformance.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/public-conformance.json) | Prior passing observations retain original evidence identities. The comparator gates the current directory-derived active set and compares new passes with these observations. |
| [supplemental.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/supplemental.json) | Installed supplemental inventory and native audit enforce component/public obligations. |
| [rollback-assets.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/rollback-assets.json) and [prior-python-rollback-run.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/prior-python-rollback-run.json) | Retention/rehearsal tools pin original packages, attempts, asset IDs and earlier receipts. |
| [native-source-fixtures.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/native-source-fixtures.json) and [producer-cohort/](https://github.com/elong0527/yamaa/tree/main/rust/qualification/producer-cohort) | Staging and cohort comparison preserve independent fixture membership/truth. |

No active reader uses `rust/planning/`: its live API inventory moved atomically
to `rust/qualification/release-api.json` in #2161. Historical
[Python retirement](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/python-retirement-inventory.json)
and [validator assertion](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/validator-assertion-disposition.json)
maps are retrievable at their original revision; #2289 must reconcile every
unique replacement with final installed evidence. Git retrieval proves map
availability, not replay of external artifacts. Current normative ownership and
schema navigation use [the current rules](articles/rules.md), schema sources and
the independent owner controls above. No active reader requires the retired
[migration map](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rules/migration.yaml);
its former aliases, retirement decisions and prose provenance remain in Git.
The original native audit for [run 38497098490, attempt 1](https://github.com/elong0527/yamaa/actions/runs/38497098490)
at source `89b1fa90b0edfa03bc367f73426b630b36b20e85` remains in
[artifact 11866226197](https://github.com/elong0527/yamaa/actions/runs/38497098490/artifacts/11866226197).
[Offline replay 38545207097](https://github.com/elong0527/yamaa/actions/runs/38545207097)
retrieved that exact ZIP and reproduced its original `qualification.json` bytes
with the original reader. The reader at `140d936b37a40e0d4250983372f38079189b1b0a`
preserved all 97 lane identities and verdicts: 42 pass, 49 fail and 6 missing.
The original record remains incomplete, with its captured run metadata unchanged;
this historical replay does not qualify a new source or replace capacity inputs.
The 17 dated raw measurement files were deleted by
[#2345](https://github.com/elong0527/yamaa/pull/2345), following the maintainer's
finite disposition recorded in #2288/#2289. Historical
[measurement reports](https://github.com/elong0527/yamaa/tree/558d3671d920872bb8b509cc726eaf5fd94f21fd/rust/measurements)
remain retrievable from Git. The old capacity evidence tied to unavailable
original inputs is abandoned by that decision. Add new evidence to PR/issues
and this compact index, without new per-issue
ledgers or rewriting previous observations.

## Rollback and artifact handoff

The retained receipt reports earlier Python 18-tuple rehearsal
[38460509078](https://github.com/elong0527/yamaa/actions/runs/38460509078) and R
same-archive three-host rehearsal
[38435902670](https://github.com/elong0527/yamaa/actions/runs/38435902670).
They keep their preparation/source identities and do not approve 0.4.0 transition.

| Retained package | Identity and availability |
| --- | --- |
| Python 0.3.0 | Published source tag `yamaa-py-0.3.0`, exact source `845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`. Draft release 411980272 retains source asset 637944563, three source-built platform wheels, original lock/dependency pins and raw receipt bundle 638657402. Those wheels were never published prior binaries. |
| R 0.3.0 | Installed pre-cutover candidate at `a1a9c2d216075f331ac108aa4d3638573f103cea`, draft release 411979803, source asset 637944247 and receipt asset 637944307. No prior R release was published. |

Authenticated review can fetch these unpublished source packages by asset ID:

```sh
mkdir retained-prior-packages
gh api repos/elong0527/yamaa/releases/assets/637944563 \
  --header 'Accept: application/octet-stream' \
  > retained-prior-packages/yamaa-0.3.0.tar.gz
gh api repos/elong0527/yamaa/releases/assets/637944247 \
  --header 'Accept: application/octet-stream' \
  > retained-prior-packages/yamaa_0.3.0.tar.gz
```

Use a GitHub identity authorized to read the drafts. Public release-download URLs
for these drafts are unavailable. Preserve original bytes, API asset references,
calling code and complete dependency locks. In a fresh compatible prior Python
study checkout, keep `yamaa==0.3.0` and select the downloaded archive:

```toml
[tool.uv.sources]
yamaa = { path = "retained-prior-packages/yamaa-0.3.0.tar.gz" }
```

Then `uv lock` and `uv sync --locked`; keep original and restored locks. Use
Rust 1.90.0, pinned maturin 1.9.6 and the prior Cargo lock. For R, use R 4.6.1
and an empty library with `R CMD INSTALL --library=<fresh-library>` against the
retained archive; Windows requires Rtools45 and `x86_64-pc-windows-gnu`.
Study-specific complete renv/uv restoration remains a separate replay check.
Before reading study data or invoking callbacks, verify the installed version:

```python
from importlib.metadata import version
assert version("yamaa") == "0.3.0"
```

```r
stopifnot(identical(as.character(packageVersion("yamaa")), "0.3.0"))
```

Rollback starts a new whole run; earlier saved outputs retain their original
provenance.

After the remaining #2288 gates pass, record the exact final source, run/attempt,
package names/bytes and artifact identities in the release handoff. The rollback
receipts above are already re-admitted. Renew evidence affected by later
changes. The 0.4.0 source filenames and install templates identify preparation;
qualified download URLs and publication identities remain pending.
