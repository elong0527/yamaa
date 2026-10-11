# Rust migration release readiness

This is the finite blocker register for [#1742](https://github.com/elong0527/yamaa/issues/1742).
It records evidence and decisions still needed, not approval to release or switch
backends. The [2026-10-10 maintainer decisions](maintainer-decisions.md) settle
eight implementation choices, including required Windows R and the Cargo
packaging exception, and the subsequently approved first-release support matrix
and explicit exclusions. The register below distinguishes those approvals from
remaining implementation and qualification.

Historical foundation: main `3895017b619642d4cda1d14b0000e20dc65becd7` includes #1811's
combined packages and #1812's complete public cohort reports. Native run
37805429480 passes all seven jobs, all thirteen workflow checks and a clean
full 13-file review, with fresh package/source auditing and independent replay
of all 72 downloaded cohort tuples. Both Unix hosts compare all seventeen
complete original public reports; the 945-row assisted inventory remains
separate. The older matrix below retains historical #1792 evidence. See
[the current migration assessment](migration-status.md) for later qualified
public frontend, environment and producer-build slices. Cross-target checks and
local prototypes do not settle migration or release acceptance.
Follow [the delivery order](migration-order.md).

## Declared API inventory

[release-api.json](release-api.json) records 438 declared names in 23 source files:
276 Python `__all__` entries, 88 native Python stub symbols/members, 22 legacy R
exports, and 52 native R exports/S3 registrations. Re-exports are counted at each
public location. Private modules and native handles are included for removal
tracking; their inclusion is not a public compatibility promise.

Every source group has a disposition, linked issue and rationale; exceptions are
explicit per-name overrides. `replace` and `remove_public` are implementation
plans under #1751/#1757/#1758, not completed removals. `retain_candidate` and
`decision_required` need release decisions and tests. The five export entries in
`io/_descriptors.py` are internal. No old language is silently aliased to the new
language. #1751 intentionally requires no backward compatibility for the replaced
Python domain options or legacy R package.

Run `python rust/tools/check_release_api.py` to reject drift between source exports
and the inventory. The same check runs in the existing tooling test discovery.
It parses source without importing Python, R or native packages. New names,
removed names, new export files and dynamic `__all__` declarations require an
explicit inventory update. It neither generates approval nor tests API behavior.

| Surface | Required disposition and evidence |
| --- | --- |
| `yamaa.yamaa_domain`, `DomainRun`, `DomainRunError`, `domain.py` | Replace with `yamaa.domain`/`check`, `DomainError` and the #1751 result contract; rename the colliding module; installed end-to-end tests must prove failure/output/log/save behavior. |
| `DomainRun.inputs`, `.input`, `.spec`, `issues.severity`, `save(path)` | Remove under #1751; inspect properties, signatures and docs as part of B02, beyond the declared-name guard. |
| `generate_study_document` and submission exports | Replace with `define` only after #1758's three submission fixtures and exact XML/JSON outputs qualify. |
| Planner, dispatcher, expression, runtime, model, verification and function re-exports | Remove public semantic-extension surfaces after their consumers migrate; preserve the reference baseline until transition. |
| Function artifact/contract/cache helpers | Replace under #1757; ordinary locked function packages and engine ports replace artifact resolution and cached activation. |
| IO, custom-schema/provenance, ODM and authoring helpers | Explicit decisions remain. `read_odm`, `write_odm_parquet` and package version metadata are retention candidates, not evidence of qualified parity. |
| All 22 `cdiscbuilder` exports | The target is package replacement. `parse_odm_to_long_df` has an explicit unresolved translation/removal decision; no equivalence to `read_odm` is assumed. |
| Native Python/R probes, private handles and scalar S3 methods | Migrate qualification consumers, then fold native distributions into `yamaa`. The approved full-range `yamaa_int64_vector` representation, separate missingness and exact-or-error conversions still need installed qualification. |

The inventory covers declared exports, not every importable submodule name,
inherited class member, Python import side effect or R internal function. B02/B03
remain open until installed attribute/signature/result checks and the retained
helper decisions cover those surfaces. This register does not close the API
inventory acceptance box merely because the declared-name check passes.

## Installed evidence and support decisions

[Run 37702319587](https://github.com/elong0527/yamaa/actions/runs/37702319587)
and [its final installed-artifact audit](https://github.com/elong0527/yamaa/pull/1792#issuecomment-6049397223)
qualify the following prototype package combinations at that exact revision.
All 13 checks passed, and all native jobs passed attempt 1. Six Python forms pass
22 supplemental suites, 37 original-document methods and four schema methods
each; both R forms pass all 17 scripts with strict `Status: OK`. Exact packaged
source bytes, the 67-contract/39-suite catalog and the unchanged 945-row assisted
inventory were audited against the tested merge and its exact base/head parents.

| Host | Observed system | Package forms exercised | Evidence limit |
| --- | --- | --- | --- |
| CPython 3.14.7 | Ubuntu 24.04 runner, x86_64, glibc 2.39 | Native wheel and independent source-rebuilt wheel, installed with reference host wheel | Complete assisted-route inventory plus private shared original-document tests; no public replacement API qualification. |
| CPython 3.14.7 | macOS 15.7.9, arm64 | Same two forms | Same limit. |
| CPython 3.14.7 | Windows Server 2025 runner, x64 Python installation | Same two forms | Same limit; record runner/image changes in each release audit. |
| R 4.6.1 | Ubuntu 24.04 runner, x86_64 | Standalone native source package | 17 installed supplemental suites; no public `yamaa` R package or full benchmark route. |
| R 4.6.1 | macOS 15.7.9, arm64 | Standalone native source package | Same limit. |

The manifests declare Python >=3.12 and R >=4.6.1. The native wheel uses
`abi3-py312`; that ABI choice is not installed qualification of Python 3.12/3.13
or future interpreters. Python 3.12 numerical-probe CI is not an installed-wheel
API test. Windows R is required for the first shared Rust release by the
2026-10-10 maintainer decision (#1857); implementation and installed qualification
are tracked in #1903. The candidate's later hosted R evidence is scoped below;
the final combined public cohort remains unqualified.
The maintainer has now approved CPython 3.12, 3.13 and 3.14 plus R 4.6.1
on Linux x86_64, macOS arm64 and Windows x64, with Python wheel/source and R
source packages. Every runtime/platform/form combination is required. Other
versions, implementations, free-threaded Python, architectures, musl/Alpine and
R binary distributions are explicitly excluded from the first release by
[the support decision](maintainer-decisions.md#first-release-support-matrix).

The native CI keeps one abi3 wheel/source artifact build per platform and runs
the entire installed Python qualification separately on all three interpreters,
for both wheel and independent source-rebuild forms: eighteen installed lanes.
The public cohort comparator requires each interpreter/form's complete reports.
PortableLibmV1's exact candidate comparison requires all nine OS/interpreter
reports, including Python 3.13. These are required gates; changing the workflow
does not preemptively turn missing reports or unqualified Windows R into passes.

The native R CI job now runs `R CMD check --no-manual` on Linux and macOS in
addition to build/install and the separately recorded supplemental scripts. It
requires `Status: OK` and retains check/install/test logs plus the source revision.
This checks the prototype package and does not establish the final API or a PDF
manual qualification. Both hosted R checks in the audited #1789 run pass with
`Status: OK` and all 17 scripts passing; their check/install logs and source
revision are retained. Long Rust fixture paths were shortened to `specs/`
without changing the fixture bytes. Package-local Rust build intermediates
receive post-link cleanup, following
[rextendr's packaging fix](https://github.com/extendr/rextendr/pull/419).
The check remains enabled and still requires `Status: OK`. This scopes R's
object-symbol inspection consistently with upstream Rust/R packaging; it does
not remove Rust's standard-library abort path or prove recovery from allocation
failure, double panic or every native fault. B16 remains open. Earlier local
checks used an external Cargo target directory, so they did not reproduce the
archive-symbol finding; the audited hosted checks exercise the default
package-local target layout. B15's scope decision is settled; it remains open
for installed qualification of the final combined public package across every
approved tuple.
The macOS Rust archive build defaults its deployment target to 11.0 while
preserving an explicitly supplied target. This covers Rust and its C codec
objects; R's own compilation/link settings remain supplied by R. The audited
macOS check has no archive deployment-target warning.
Release scope must either add the appropriate checks or record an explicit
approved exclusion. Package versions in the audited evidence are Python host
0.2.0, native Python/R 0.1.0 and core 0.1.0; none constitutes the final combined
package version policy.

The earlier #1912 head `27af5559` is tested as `90f3cfd6` in native run
38124033923. Its retained Linux/macOS/Windows R 4.6.1 source lanes each pass all
23 ordered installed suites and strict `Status: OK`. Independent R producer
report/artifact truth and source-byte audits pass; the Windows installed DLL
also passes x64 GNU/UCRT and sole-initialization-export checks. This qualifies
those actual R lanes. The eighteen Python lanes, full public/producer replay
and current-main rounding/log integration still need their final gates. The
older prototype table above retains its original scope.

Final common head `91e645e3` is now tested as `7b0852dd` in native run
38133741554, with tested base `8e97023f` and exactly the head's tree.
Its actual Windows R source lane passes all 24 ordered installed suites, strict
R 4.6.1 `Status: OK`, complete public/graph reports, all twenty-four original
source inputs and the retained x64 GNU/UCRT DLL audit. Windows native tests pass
1,280 tests per mode and 111 tooling tests. All six Windows Python lanes fail an
authored graph-report path expectation at this source. Independent canonical-path
replay passes their retained native reports; corrected installed packages,
remaining lanes, complete public/producer replay and newer-main composition
still prevent qualification of the full common matrix.

## Finite blocker register

The rows distinguish approved policy from open implementation and release
qualification. B14's packaging policy is resolved; its build evidence remains
under B13. No open row is preemptively reclassified as unsupported release scope
or a future enhancement. A disposition that narrows scope requires an explicit
linked decision. Issue numbers identify the implementation/test or decision owner.

| ID | Blocker and current evidence | Owner | Evidence required to close |
| --- | --- | --- | --- |
| B01 | #1920 and #1927's audited eighteen Python lanes and installed Linux/macOS R evidence pass 528 comparisons across all eleven public cases, including the five historical timing fixtures; 66 Windows R comparisons remain explicitly missing in each. Intervening six-case replays retain their narrower scope. The final combined matrix remains open. | #1739, #1751, #1903 | Preserve complete original-file observations and saved bytes, block reference semantics, and qualify all required Windows R tuples. Promote only evidenced `shared_run` tuples. |
| B02 | Public domain/check/result/log/save API and benchmark runner migration. | #1751 | Installed signatures, wrong-argument exceptions, issue rows for specification/data/environment/function failures, logs retained on failure, explicit save, and every supported Python/R runner pair migrated. |
| B03 | Issues-only checks, JSON-text context and full-range `yamaa_int64_vector` are approved. #1918/#1922 qualify exact conversions, missingness, subsetting and comparisons in installed Unix R. Final Windows R, retained helpers, NUL-string disposition and full surface review remain. | #1751, #1742 | Qualify the approved representations, separate missingness and exact-or-error integer conversions. Settle custom schema/provenance and IO/ODM/style helper dispositions; test retained surfaces and deliberate removals, including class members and undocumented imports. |
| B04 | #1919 merged declaring-file public paths after final review, complete hosted checks and source/installed-byte audits. All eighteen Python lanes and installed Linux/macOS R qualify; final combined Windows R remains required. | #1905, #1751, #1755 | Retain original declaring-file provenance, no fallback/configuration search, absolute and parent-relative local paths without yamaa root containment, kind/link/snapshot checks and resource preflight in the final combined matrix. |
| B05 | Combined `yamaa` packages qualify in #1811; obsolete probe/duplicate cleanup remains open. | #1751, #1754, #1742 | Clean wheel/source and R package installations exercise combined artifacts; remove duplicate/probe exports only after their replacement consumers qualify. |
| B06 | #1757 is closed by #1856 for public environment/function admission, lock verification, per-build activation and five equivalent Python/R fixtures. | #1757, #1742 | Retain that bounded qualification and exercise the final installed release matrix, including Windows R and later public workflow/submission integrations, without weakening activation-before-data. |
| B07 | #1927 qualifies producer-first primary Save, whole-set target admission, retained failure prefixes and held-byte retries under the approved #1866 lifecycle. Formal original producer M1 reports, graph sidecars and the final Windows R integration remain open. | #1741 | Producer-once execution, activation before study data, exact serialized/rounded consumer input and complete ledgers. Build without writes; explicit Save publishes held bytes producer-first without rereads/callbacks/re-encoding. Failed publication stops dependents and preserves prior writes and completed execution evidence. |
| B08 | Environment codelist admission is qualified under #1757; #1916 qualifies source-system-version override/default. #1925 qualifies shared Define-XML/Dataset-JSON generation in all eighteen Python and both Unix R lanes with fixed-time DM and independent AE/VS structure/XSD checks. #1947 value metadata, complete final fixture truth and old-format retirement remain open. | #1758, #1742 | Qualify final terminology scopes and the define API. Retain the optional section override and installed-version default; all three migrated fixtures use explicit `"1.0"` and preserve fixed-time XML/JSON truth before retiring old formats. Record actual generator versions separately. |
| B09 | Remaining language/compiler families. Component support is not full-language compilation. | #1585, #1752, #1754 | Qualify handlers, row-output dependencies, predicates/flags/first-available, correlated/keyed aggregates and lookups, full verification scopes, strings/regex/temporal/ODM and submission/value metadata from the executable inventory. |
| B10 | #1919/#1927 qualify native declaring-file paths and ordered producer primary publication with original held bytes and failure prefixes. Remaining codec combinations, graph sidecars and final combined platform qualification remain open. The older #1785 callback codec audit retains its narrower scope. | #1755, #1585 | Exact CSV, logical Parquet/ordinal/type contracts, producer re-ingestion, non-regular/missing/changed resources, atomic publication and publication-failure behavior through actual file ports. |
| B11 | Complete diagnostics and failure timing. Core owns preflight, output-declaration, numeric/aggregate grammar, binding/dependency, CSV profile/declared typing, closed Parquet metadata, window/lookup and predicate grammar findings; #1790 qualifies original scalar literals and conversion wrappers; schema kinds are portable after #1789; #1772/#1773 merged captured schema and classified source observations. #1932/#1928 add classified environment-lock and dataset-rounding findings; #1934/#1937 add borrowed schema/inheritance projection after complete context admission. Remaining families and final file-port qualification remain open. The eleven-case public gate includes the five historical timing fixtures; their older assisted-route mismatches remain visible. | #1753, #1739 | Preserve normative phase, order, requirement, authored path/context and full source/check/handler ledgers; require the five timing fixtures through complete installed public reports against independent truth on the final matrix, without masking the historical assisted gaps. |
| B12 | #1926 closes #1740's PortableLibmV1 decision ledger with exact candidate comparison, independent accuracy evidence and documented historical differences. The original ADEG filtered aggregate remains `unsupported_operation`; full affected scopes and final release qualification remain open. | #1740, #1742 | Qualify every affected final scope and retain reviewed disposition for observable numerical/artifact differences. Public APIs and YAML do not select policy/backend. |
| B13 | #1911 merged canonical Cargo locking, unchanged source-package workspaces and locked builds after complete hosted and immutable-byte audits. #1925 retains all nine Define original files. Root `783ad399` source packages retain twenty unchanged original files, including the two compile-time ordered-sum inputs. Actual unpacked-source all-target Clippy freshly checks all five crates; missing-original controls and portable include refusals pass. Final common Windows head `91e645e3` retains twenty-four originals. Complete final installed/platform evidence remains open. | #1906, #1742 | Retain clean full-transitive-graph build evidence with exact versions/features/toolchains on all required platforms, including Windows R. |
| B14 | Packaging checksum policy resolved: AGENTS.md permits python/uv.lock and the canonical rust/Cargo.lock with unchanged staged copies. | #1742, #1906 | No further policy decision is needed for those files. B13 tracks implementation/build evidence. Preserve held-byte comparison; runtime identity, contract, activation-cache and resource-snapshot digests remain prohibited. Additional exceptions need a separate decision. |
| B15 | #1917 records the approved matrix/exclusions: CPython 3.12-3.14 and R 4.6.1 on Linux x86_64/macOS arm64/Windows x64, Python wheel/source and R source. Eighteen Python lanes qualify for that slice. #1912's earlier tested `90f3cfd6` passes all three R lanes, 23 installed suites each and strict checks. Common tested `7b0852dd` includes its tested base/graph/value changes with a 30-Python/24-R catalog; its Windows R 24-suite/strict/source/DLL evidence now passes. Six authored Windows Python path-expectation failures, corrected packages, remaining lanes, full public/producer replay and newer-main composition remain open. | #1903, #1742 | Complete all eighteen installed Python interpreter/platform/form lanes and all three R source lanes, including Windows R native file ports, public/cohort suites and strict R CMD check. Retain exact runtime, system and artifact provenance; the approved target is not completed qualification. |
| B16 | Native ownership, interruption, reentrancy and repeated-use behavior across the final facade. Probes already exercise many cases. | #1755, #1742 | Repeat those contracts through final installed domain/check/workflow/save APIs, including failed builds and expired/mismatched handles; probes alone do not close this. |
| B17 | Representative phase/performance/memory evidence. Existing harness is small and cold, with host-assisted preparation. The [bounded public collector](shared-public-measurement-plan.md#bounded-collector-and-replay) passes an independent actual local smoke: twelve children/eighteen Builds, 54 reports, 36 exact CSV comparisons and 156 losslessly retained raw clock boundaries. Earlier failures remain retained, with broader workloads, phase/callback attribution, R/Windows RSS and agreed-scope characterization still open. | #1742 | Shared-path workloads at agreed sizes, repeated cold/warm measurements, raw samples/spread, exact truth/callback checks, phase attribution and peak RSS or explicit unavailability. |
| B18 | Performance regression budgets and support tradeoffs. The [measurement plan](shared-public-measurement-plan.md#memory-and-budget-decision) identifies the required decision inputs; its suggested sizes supply no approval or threshold. | #1742 | Maintainer-approved numeric budgets before release qualification; evaluate distributions and representative workloads, not one smoke sample or a tiny fixture speedup. |
| B19 | Qualified transition, rollback and evaluator removal. | #1585, #1742 | Explicit transition approval after all supported matrix/API/fixture gates pass; rollback pins a prior release in the host lock; delete duplicate evaluators and update installation/docs/CI deliberately. |

The five timing gaps are named in
[known-gaps.json](../qualification/known-gaps.json): `negative-formula-flag`,
`negative-row-aggregate`, `negative-row-no-prior`, `negative-source-missing-field`
and `negative-source-trivial-filter`. The inspected inventory has 315 reference
passes, 76 assisted native passes, 234 Unsupported results, those five mismatches,
and 315 unexercised R tuples. The private route subsequently passed these documents with independent
complete reports and actual ingestion timing. Both public installed suites
also compare all five; they are now required by the eleven-case M1 gate.
Shared reports retain their actual route and do not promote or remove
the older assisted inventory rows or baseline mismatches. Three additional
predicate requirement/context gaps are documented in `predicate_diagnostics.md`
and remain unqualified. These are baseline route counts, not migration
percentages or evidence that 234 independent compiler features are missing.

## Measurement preparation

Follow the [shared public measurement plan](shared-public-measurement-plan.md).
The [existing phase harness](../PHASE_MEASUREMENTS.md) retains historical
host-normalized measurements. Proposed workloads below need agreed
sizes and budgets before they become release gates:

| Workload | Candidate dimensions | Independent checks at every size |
| --- | --- | --- |
| Lookup | Base keys, donor multiplicity, unmatched keys and correlated selections | Complete values/order, stable first/last ties, exact bytes and donor/handler counts. |
| Ordered aggregate | Rows per group and group count, including cancellation and missing values | Source-order SUM/MEAN behavior, missingness, grouping identity and exact artifact checks. |
| Windows | Partition count/size, ties, missing runs and offset distances | Rank/offset/previous-non-missing/locf/baseline semantics and stable order. |
| Host callbacks | Call count, null short-circuit, result types and argument width | Exact call traces, originals of errors/interrupts, per-build tests and no activation cache. |
| Producer workflow | Chain depth, fan-out, shared producers and serialized consumer sizes | Producer once, activation-before-data, serialized rounding, failure ledgers and save gates. |

A starting proposal is 1,000/10,000/100,000 driving rows with separately varied
partition/donor shapes. This is a proposal, not a budget or supported capacity.
Do not generate truth from the candidate engine. Retain original fixed fixtures
and author independent scalable expectations before measuring. Report cold
process startup/import separately from warm repeated runs, and compilation,
ingestion/conversion, execution, verification and rendering separately where
measured. Keep raw samples, exact revision/package/environment and spread. Missing
metrics remain null/unavailable; do not label process-lifetime RSS as per-phase
memory. Do not measure while this checkout is concurrently building packages.

## Completion and updates

A blocker closes only with linked authoritative evidence or an explicit scope
and decision record. #1742 may close its planning decisions only when each row has
that disposition; #1585 still requires actual supported release and transition
qualification. Keep this register and the declared-name inventory current as
APIs migrate. The export checker and green tests cannot make a pending decision
or authorize default cutover.
