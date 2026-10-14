# Shared Rust release

Reviewed on 2026-10-13 (America/New_York) against main
`d8cb226a613f0ee26cac400e2baeecbbf2ba11cd`. This page is the current decision and gate home
for [#1969](https://github.com/elong0527/yamaa/issues/1969). A merged change, a
qualified source revision, an accepted decision and a published asset are
different states.

## Current state

The shared language engine, Python/R domain/check/Define APIs, benchmark runners,
native repository tooling and coordinated 0.4.0 source versions are merged.
The old Python evaluator, probe/reference routes and R `cdiscbuilder` are removed.
The performance harness in [#2137](https://github.com/elong0527/yamaa/pull/2137)
is merged at `c6376e36`; complete installed campaign acceptance remains pending.
Current main also merges the
[#2167 exact-run/attempt admission fix](https://github.com/elong0527/yamaa/pull/2167),
[#2164 R check dependencies](https://github.com/elong0527/yamaa/pull/2164),
[#2165 dependency guard repair](https://github.com/elong0527/yamaa/pull/2165),
[#2168 IPC adapter guard repair](https://github.com/elong0527/yamaa/pull/2168)
and [#2166 exact downstream math controls](https://github.com/elong0527/yamaa/pull/2166).
These repairs require current-source qualification; merging them does not
establish a passing final campaign or combined-main audit.
Removal and version changes landed **before** final item-10 budgets and item-11
transition acceptance. Historical language work is complete; final evidence must
qualify the actual combined source rather than relabel an earlier branch run.

The active ratchet requires 316 of 318 authored cases in every installed tuple;
the two explicit historical exclusions remain declared in
[public-conformance.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/public-conformance.json).
Its historical qualification is revision `2ba10418607933f92597f7c00ac363a3113a49db`,
[run 38416920074](https://github.com/elong0527/yamaa/actions/runs/38416920074),
attempt 1. It does not qualify today's 0.4.0 source. Final source/run/attempt,
original artifact identities, independent replay and publication handoff belong
to [#2145](https://github.com/elong0527/yamaa/issues/2145) and
[#2147](https://github.com/elong0527/yamaa/issues/2147).

GitHub release metadata checked on the review date shows published
[Python 0.3.0](https://github.com/elong0527/yamaa/releases/tag/yamaa-py-0.3.0)
(release 409220247, no attached binary assets). The retained Python packages
and pre-cutover R 0.3.0 source candidate are **unpublished drafts**. No previous
R release or shared 0.4.0 release is published. Source version numbers and green
CI do not publish a release. Recheck release metadata and actual assets at handoff.

## Approved policy

[#1973](https://github.com/elong0527/yamaa/issues/1973) records decisions 1-9
and 12-19, with the [approval comment](https://github.com/elong0527/yamaa/issues/1973#issuecomment-6112883568)
and [original decision source](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/maintainer-decisions.md)
as provenance. These approve target behavior; final installed qualification is
separate.

| Decision | Current policy |
| --- | --- |
| 1/2: public APIs | Python `domain`, `check`, `define`, result/error types, version metadata and self-contained `read_odm`/`write_odm_parquet`; R `yamaa_domain`, `yamaa_check`, `yamaa_define`. Keep the style CLI with private implementation. Legacy/probe/`cdiscbuilder` APIs are retired. |
| 3/4: host values and arguments | Lossless R i64/NUL carriers with separate missingness and preserve-or-error conversion. Python `environment` is keyword-only; wrong argument types raise `TypeError`. R argument errors are classed. A missing specification is an issue row. |
| 5-7/9: transport and diagnostics | No JSON request/reply callback protocol; issue context remains JSON text. Registry-owned conditions and phases; only `unsupported_operation` and `engine_rejected` have reviewed null requirements. Assisted inventory/probe routes are retired. |
| 8: qualification and ownership | Complete installed native matrix, including Windows R, before merging native/R/packaging/native CI changes. One current narrative and no new per-slice history ledgers. |
| 12-14: resources and distribution | Resolve paths from the declaring file. First R distribution is a standalone GitHub source tarball; CRAN/r-universe are later work. No public resolved-spec producer; historical outputs were removed at cutover. |
| 15-19: language truth | Finite strictly ascending `cut` breaks and one label per interval; registered CSV empty-string refusal; static greatest/least comparability and first-source type on ties; signed-i64 predicate overflow; row filters read their specified completed non-window/grouped derivations. Preserve independent expected truth. |

The [API inventory](https://github.com/elong0527/yamaa/blob/main/rust/qualification/release-api.json)
and `rust/tools/check_release_api.py` declare and check symbol dispositions;
inventory entries marked candidate/internal do not promise independently supported
private APIs. Host usage and installation are in the
[Python](https://github.com/elong0527/yamaa/blob/main/python/README.md) and
[R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) guides.

### Numerical behavior

The approved internal `PortableLibmV1` policy pins `libm` 0.2.16 with default
features disabled for shared EXP/LN/POWER, Rust 1.90.0, binary64 operations,
written association, round-to-nearest/ties-to-even and enabled subnormals.
One locked yamaa release supplies one behavior; public APIs/YAML select no math
policy or backend. Historical Python platform results can differ in exact bits,
dependent predicates, conversions, handlers and saved bytes. Those compatibility
changes were accepted; universal correct rounding or a global accuracy bound
was not claimed. Changing the implementation/toolchain/target requires renewed
qualification; observable contract changes require a release decision.

Current [portable math goldens](https://github.com/elong0527/yamaa/blob/main/rust/crates/yamaa-core/tests/fixtures/portable_math.tsv)
are shared by Rust and Python checks. The original
[numerical decision and witnesses](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/1740-math-decision.md)
and its [retained evidence](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/qualification/math-decision-v1.json)
remain in Git. Cleanup #2162 removed that old JSON and certificate readers from
the active tree; the new golden test does not independently replay those old
accuracy certificates. Final qualification still needs nine math assessments
and their exact cross-platform comparison.

### Platforms, capacity and budgets

Supported qualification is 18 CPython tuples: 3.12/3.13/3.14, wheel and independent
source rebuild, on Linux x86_64, macOS arm64 and Windows x64; plus R 4.6.1 source
installation and strict `R CMD check` **Status: OK** on the same three platforms.
R sources declare Rust 1.90.0/Cargo and include the unchanged canonical lock; Windows
requires Rtools45 and the GNU Rust target. Other runtime versions or Python
implementations, free-threaded Python, other architectures, musl/Alpine and R
binary distributions are explicitly excluded from this first release. The
qualification images are Ubuntu 24.04/glibc, macOS 15/arm64 and Windows x64;
record actual image/runtime versions rather than promising additional baselines.
Only packaging-tool checksums in
`python/uv.lock`, `rust/Cargo.lock` and unchanged staged copies are allowed;
runtime/contract/cache/resource identity digests remain prohibited.

Capacity constants remain unapproved. [#2142](https://github.com/elong0527/yamaa/issues/2142)
must justify adopted bounds, units, counters and actual refusal behavior using
installed Python/R approved-size correctness and complete native qualification.
Current bounds charge logical rows/cells/work, captured bytes, graph ownership,
encoded reports and publication prefixes separately; they are not RSS guarantees.
Item 10 remains undecided: [#2144](https://github.com/elong0527/yamaa/issues/2144)
collects 27 cases across all 21 tuples at 1,000/10,000/100,000 rows, with 10 cold
samples, 30 warm samples and three retained warmups. Same-machine previous
release/reference baselines and original observations must support the chosen
thresholds. [#2143](https://github.com/elong0527/yamaa/issues/2143) independently
recomputes coverage/distributions and evaluates versioned budgets. A capacity
refusal is a finding, and an unavailable/incomparable metric needs a reason;
neither becomes a passing timing or memory sample.

### Rollback and transition

Rollback restores compatible prior calling code, complete study locks and exact
prior artifacts in a fresh environment, checks the installed version before any
study input, then starts a new whole run. It never changes backend during a
running build, callback or Save; earlier saved outputs retain their provenance.

[rollback-assets.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/rollback-assets.json)
records byte-equality receipts and asset identities. Python source revision
`845a91ff9ba4ab6599efbd79fafe1c4e1dd14255` is the published 0.3.0 source;
its retained wheels are source-built rollback artifacts. All 18 prior Python
wheel/source reinstalls passed in
[run 38460509078](https://github.com/elong0527/yamaa/actions/runs/38460509078),
attempt 1; [prior-python-rollback-run.json](https://github.com/elong0527/yamaa/blob/main/rust/qualification/prior-python-rollback-run.json)
pins its exact preparation/source/artifact identities. Durable retention was
verified in [run 38488955022](https://github.com/elong0527/yamaa/actions/runs/38488955022).
The identical R source candidate at
`a1a9c2d216075f331ac108aa4d3638573f103cea` passed all three fresh-library
reinstalls in [run 38435902670](https://github.com/elong0527/yamaa/actions/runs/38435902670).

The current durable source assets are Python release 411980272 / asset 637944563
and R release 411979803 / asset 637944247. Python receipt bundle 638657402 retains
the original 22 artifact ZIPs; R receipt asset 637944307 retains its earlier
same-archive replay. Both releases remain drafts; public
release-download URLs are not usable publication pins. Authorized review uses
authenticated release-asset downloads and compares actual bytes with the retained
receipts. The [original exact pin recipes](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/rollback-pins.md)
remain in Git. Fetch the source bytes using an identity authorized to read drafts:

```sh
mkdir retained-prior-packages
gh api repos/elong0527/yamaa/releases/assets/637944563 \
  --header 'Accept: application/octet-stream' > retained-prior-packages/yamaa-0.3.0.tar.gz
gh api repos/elong0527/yamaa/releases/assets/637944247 \
  --header 'Accept: application/octet-stream' > retained-prior-packages/yamaa_0.3.0.tar.gz
```

In a fresh compatible prior Python study checkout, preserve the calling code,
`yamaa==0.3.0` and every other dependency pin, then select the verified archive:

```toml
[tool.uv.sources]
yamaa = { path = "retained-prior-packages/yamaa-0.3.0.tar.gz" }
```

Regenerate the selected source record with `uv lock`, then `uv sync --locked`;
retain both the original and restored study locks. The source backend pins
maturin 1.9.6; use Rust 1.90.0 and the unchanged prior Cargo lock. R restoration
uses the compatible prior `renv.lock` and a fresh project library; authenticated
review can install the fetched archive with `R CMD INSTALL --library=<fresh-library>`.
Verify Python distribution version or R `packageVersion("yamaa")` equals `0.3.0`
before reads or callbacks. Study-specific complete-lock restoration remains a
separate check from the earlier package rehearsals.
Published study-lock restoration, the full producer graph Save baseline and
current asset accessibility still require acceptance under #2147/#1999.

Item 11 remains pending. #2147 may record transition acceptance only after
combined-main language/API/platform/math/reproducibility evidence, adopted
capacity/budgets, rollback and exact artifact handoff pass. Record the actual
removal-before-acceptance chronology; never backdate approval. Release tagging,
package publication and asset replacement are separate operations.

## Final gate ownership

Implementation rows below are merged and require final-source renewal. Earlier
qualified runs retain their own revision/attempt/scope. #2145 owns the genuine
post-removal combined-main audit; #2147 reconciles final acceptance and closes
#1969 last.

| Gate | Current disposition and final owner |
| --- | --- |
| B01: language corpus | 316 active cases and two exclusions declared; final 21-tuple renewal: #2145. |
| B02: domain/check API | Shared facades merged; complete installed contract renewal: #2145. |
| B03: host surface/values | i64/NUL and helper/retirement policy decided; final surface/conversion renewal: #2145. |
| B04: resources | Declaring-file paths, original failures and platform controls merged; final renewal: #2145. |
| B05: probes/callbacks | Reference/probe/JSON protocols retired; assertion replacements and final contracts: #2145/#2147. |
| B06: selection | Ordinary/environment/producer selection merged; final priority/admission renewal: #2145. |
| B07: orchestration | Producer graph and failure/publication order merged; final renewal: #2145. |
| B08: Define | Shared orchestration and submission ownership merged; final renewal: #2145. |
| B09: language families | Original language WPs closed; final independent conformance renewal: #2145. |
| B10: codecs/publication | File/CSV/Parquet/held-Save implementation merged; exact bytes/failures renewal: #2145. |
| B11: diagnostics | Registry and reviewed null ownership merged; final corpus/guard renewal: #2145. |
| B12: numerical policy | PortableLibmV1 approved; final math/public contract renewal: #2145. |
| B13: package reproducibility | Canonical locking/staging merged; actual source membership, archive bytes and toolchains: #2145. |
| B14: digest policy | Packaging-only exceptions decided and enforced; final build evidence: #2145. |
| B15: installed audit | Exact final source/run/attempt/artifact independent audit pending: #2145, handoff #2147. |
| B16: lifetime/failures | Owned results, interruption/reentrancy/repeated calls merged; installed renewal: #2145. |
| B17: measurement/capacity | Harness merged; admission/replay #2141, adopted bounds #2142, full campaign #2144. |
| B18: budgets | Item 10 undecided; independent auditor #2143, evidence and decision #2144. |
| B19: transition/rollback | Prior artifacts retained in drafts; final entry gates/item 11/handoff #2147. |

The parent gates map to B01-B12/B16 (fixtures/language/workflow/API),
B12-B15 (installed matrix/numerics/builds), B17-B18 (performance/capacity),
B19 (transition/rollback), B03/B05/B13 (removal) and B15/B19 (exact handoff).
#2146 owns this narrative; #2147 updates it from final evidence afterward.

## Current records and historical cleanup

[#2154](https://github.com/elong0527/yamaa/issues/2154) must inventory live
readers before further cleanup. #2161 already moved `release-api.json` and its
checker/test to `rust/qualification/`, then removed `rust/planning/` and root
Rust architecture/status documents. Retired Python/validator assertion maps have
no current path readers; the original
[Python retirement map](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/python-retirement-inventory.json)
and [validator assertion map](https://github.com/elong0527/yamaa/blob/43bb4437d9c0890ec7877ec7575a517c771a8147/rust/planning/validator-assertion-disposition.json)
remain retrievable at revision 43bb4437. #2147 must reconcile every unique
retired assertion replacement with the final installed evidence. Git retrieval
establishes map availability, not access to external replay artifacts.
Retirement verification now uses `test_python_package_hygiene.py`, the installed
supplemental suites and source guards; restoring an old map would not qualify
those replacements.

| Live record | Current readers |
| --- | --- |
| `rust/qualification/release-api.json` | `check_release_api.py`, `test_release_api.py`; moved together in #2161. |
| `rust/qualification/public-conformance.json` | Native Python/R conformance ratchets, runner migration tool and declared matrix tests. |
| `rust/qualification/supplemental.json` | Supplemental runner/catalog checks, package staging and exact-source native audit. |
| `rust/qualification/native-source-fixtures.json` | Source-package fixture staging and package membership/hygiene tests. |
| `rust/qualification/producer-cohort/` | Staged installed inputs, public/producer comparators and native cohort selection. |
| Rollback JSON records | R archive preparation, prior Python retention and independent receipt review. |
| `rules/migration.yaml` | `check_rule_rewrite.py`, `check_requirement_registry.py`, `editorial.py`, generated references and their tests. |

`rust/measurements/` still carries pending performance findings. Delete dated
raw evidence only after durable original
source/run/attempt/artifact inputs are retrievable and independently replayable.
The [CI audit/replay instructions](https://github.com/elong0527/yamaa/blob/main/.github/ci.md)
define exact-source evidence, including the nonblocking main audit's independent
verdict. Git history preserves deleted prose; it does not establish access to
expired Actions artifacts or final acceptance.
