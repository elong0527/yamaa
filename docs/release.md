# Release and migration

Reviewed 2026-10-13 (America/New_York) against main
[`1b795ca4`](https://github.com/elong0527/yamaa/commit/1b795ca4), the current
[#1969](https://github.com/elong0527/yamaa/issues/1969) and
[#1973](https://github.com/elong0527/yamaa/issues/1973) gate records, retained
qualification records and the authenticated GitHub release listing. This is the
current narrative; qualification observations keep their own tested revisions.

The [final release handoff](release-handoff.md) reconciles every entry gate,
independently re-admitted rollback receipts, measured-source renewal and exact
package selection. Transition acceptance and the final package selection remain
pending there until all evidence gates pass.

## State

| State | Observed scope |
| --- | --- |
| Implemented and merged | Shared language execution; Python/R domain/check/define; runner conversion; legacy evaluator/API/probe and R/cdiscbuilder removal; coordinated Python/R/Cargo 0.4.0 source versions. The performance harness is merged through PR #2137; subsequent native dependency/guard, portable-math control and performance admission repairs are merged through PRs #2164-#2168. |
| Qualified at earlier revisions | Retained public-conformance ratchet: run 38416920074, attempt 1, source `2ba10418607933f92597f7c00ac363a3113a49db`; earlier rollback rehearsals below. Their results do not qualify current main or its 0.4.0 composition. |
| Pending evidence/decisions | Final combined-main installed/numerical audit, adopted capacity bounds, complete performance campaign and item-10 budgets, item-11 transition acceptance and exact artifact handoff. |
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
transition approval. [#2147](https://github.com/elong0527/yamaa/issues/2147) records
acceptance only after the remaining evidence passes. Publication/tagging/asset
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
  remain in Git. Current goldens do not replay the retired accuracy-certificate
  tools or establish universal correctly rounded behavior.
- Qualify CPython 3.12/3.13/3.14 wheel and independent source builds on Linux
  x86_64, macOS arm64 and Windows x64 (18 tuples), plus R 4.6.1 source builds on
  the same hosts (three tuples), each with strict `R CMD check` **Status: OK**.
  Other runtimes/architectures, free-threaded Python, musl and R binary packages
  are outside this first release. R distribution starts with a staged GitHub
  source tarball; CRAN/r-universe are later work.
- Keep canonical Cargo locking and unchanged source-package copies. Runtime,
  contract, cache and resource identities use references/canonical forms/bytes;
  only packaging-tool lockfiles carry their own checksum fields.

These are target decisions, not qualification results. Host behavior and source
installation commands have one home in the
[Python](https://github.com/elong0527/yamaa/blob/main/python/README.md) and
[R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) guides.

## Remaining gates and owners

The historical B01-B19 identifiers remain useful for reconciliation. Closed
implementation owners are not reopened; the following owners renew final evidence.

| Gate | Current disposition and acceptance owner |
| --- | --- |
| B01 - active conformance | 316 active of 318 authored cases, with two explicit historical exclusions; final 21-tuple admission: [#2145](https://github.com/elong0527/yamaa/issues/2145). |
| B02 - Python API | Shared facade merged; final signatures/results/arguments/helpers: #2145; export/archive reconciliation: [#2147](https://github.com/elong0527/yamaa/issues/2147). |
| B03 - R API/scalars | Approved i64/NUL/error/Define surface merged; renew installed ownership/conversions: #2145. |
| B04 - filesystem/platforms | Declaring-file paths and Windows controls merged; renew final installed source: #2145. |
| B05 - probe/JSON retirement | Removal merged; reconcile unique retired assertions and retained consumers: #2147. |
| B06 - route/environment selection | Unified composition merged; final full installed evidence: #2145. |
| B07 - producers | Orchestration merged; final port/callback/failure/publication order: #2145. |
| B08 - submission | Shared Define and registry ownership merged; final installed render/Save/default-version controls: #2145. |
| B09 - language | Language families merged; renew complete active conformance and predicate/intermediate contracts: #2145. |
| B10 - codecs/publication | Implemented; renew exact CSV/logical Parquet, original failures and held Saves: #2145. |
| B11 - diagnostics | Shared ownership and null exceptions decided; final registry/installed controls: #2145. |
| B12 - numerical | PortableLibmV1 decided; final public truth, nine assessments and exact comparison: #2145. |
| B13 - builds/locks | Canonical locks and 0.4.0 versions merged; renew actual archive membership, byte equality, features/toolchains: #2145/#2147. |
| B14 - content identity | Policy resolved by the lockfile exception above; build evidence belongs to B13. |
| B15 - independent audit | Audit implemented; passing post-removal combined-main verdict and original-input offline replay: #2145. |
| B16 - lifecycle/removal | Installed controls retained; final ownership/interruptions/reentrancy/repeated use/Save: #2145; removal reconciliation: #2147. |
| B17 - performance/capacity | Harness merged, complete acceptance pending: [#2141](https://github.com/elong0527/yamaa/issues/2141) replay, [#2142](https://github.com/elong0527/yamaa/issues/2142) adopted bounds, [#2144](https://github.com/elong0527/yamaa/issues/2144) collection. |
| B18 - numeric budgets | Item 10 remains unrecorded: [#2143](https://github.com/elong0527/yamaa/issues/2143) independent auditor and #2144 observations/budgets. |
| B19 - transition/rollback | Concrete plan and earlier rollback receipts exist; item 11, final re-admission and artifact handoff: #2147. |

For [#1969](https://github.com/elong0527/yamaa/issues/1969), fixtures/language/API
and installed/numerical gates map to #2145; performance/capacity to #2142-#2144;
transition/removal/handoff to #2147. This guide is
[#2146](https://github.com/elong0527/yamaa/issues/2146).
[#1998](https://github.com/elong0527/yamaa/issues/1998) accepts performance,
[#1999](https://github.com/elong0527/yamaa/issues/1999) accepts transition,
[#1994](https://github.com/elong0527/yamaa/issues/1994) accepts the existing cutover
and #1973 records items 10/11. Close #1969 last.

Performance acceptance is 27 cases (nine dimension variants at 1k/10k/100k) on
all 21 tuples: 567 combinations, ten cold and thirty warm samples after three
retained warmups. Use independent typed/output truth, raw clocks/RSS/phases/callbacks,
both Saves and the prior Python 0.3.0 baseline on the same machine without
concurrent compilation. Capacity constants remain unapproved. Logical rows/cells/work, captured bytes,
graph ownership, encoded reports and publication prefixes have separate bounded
scopes. Retained admission counters are per owner and do not cap whole-process
RSS. Refusals and
unavailable/incomparable metrics require explicit dispositions; do not choose
thresholds merely to make results pass.

## Qualification and retained records

Use [CI guidance](https://github.com/elong0527/yamaa/blob/main/.github/ci.md)
for sampled PR coverage, the full native route and independent audit/replay.
Ordinary dispatch supplies the installed matrix and component numeric checks;
shared Rust, nine math assessments/comparison and audit require main or explicit
qualification pushes. Main's audit is nonblocking: inspect its verdict and replay
the original inputs. Branch qualification stays `reviewed-branch`; only genuine
postmerge evidence is `combined-main`.

The live inventory is:

| Record | Reader and purpose |
| --- | --- |
| [Current rules](articles/rules.md) and [schema-navigation.tsv](https://github.com/elong0527/yamaa/blob/main/.github/scripts/yaml-validation/fixtures/schema-navigation.tsv) | `check_rule_rewrite.py` derives current normative ownership; `test_rule_rewrite.py` checks 363 independent schema owners. `generate_rule_reference.py` derives navigation from current contracts and schemas. |
| [release-api.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/release-api.json) | `check_release_api.py` and its tests compare declared exports/dispositions. `retain_candidate` labels remain inventory classifications, not final acceptance. |
| [public-conformance.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/public-conformance.json) | Public conformance runner/comparator and native workflow enforce the required case ratchet. |
| [supplemental.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/supplemental.json) | Installed supplemental inventory and native audit enforce component/public obligations. |
| [rollback-assets.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/rollback-assets.json) and [prior-python-rollback-run.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/prior-python-rollback-run.json) | Retention/rehearsal tools pin original packages, attempts, asset IDs and earlier receipts. |
| [native-source-fixtures.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/native-source-fixtures.json) and [producer-cohort/](https://github.com/elong0527/yamaa/tree/main/rust/qualification/producer-cohort) | Staging and cohort comparison preserve independent fixture membership/truth. |

No active reader uses `rust/planning/`: its live API inventory moved atomically
to `rust/qualification/release-api.json` in #2161. Historical
[Python retirement](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/python-retirement-inventory.json)
and [validator assertion](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/validator-assertion-disposition.json)
maps are retrievable at their original revision; #2147 must reconcile every
unique replacement with final installed evidence. Git retrieval proves map
availability, not replay of external artifacts. Current normative ownership and
schema navigation use [the current rules](articles/rules.md), schema sources and
the independent owner controls above. No active reader requires the retired
[migration map](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rules/migration.yaml);
its former aliases, retirement decisions and prose provenance remain in Git.
[#2154](https://github.com/elong0527/yamaa/issues/2154) owns physical history removal
and durable replay. Raw `rust/measurements/` remains pending verified retention
of its original external inputs; a scratch path or expiring Actions URL is not a
durable archive. Add evidence to PR/issues and this compact index, without new
per-issue ledgers or rewriting previous observations.

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

After #2141-#2145 pass, #2147 re-admits rollback, records evidence-backed item-11
acceptance without backdating, and hands off the exact final source, run/attempt,
package names/bytes and artifact identities. Renew evidence affected by later
changes. The 0.4.0 source filenames and install templates identify preparation;
qualified download URLs and publication identities remain pending.
