# Rust migration release readiness

This is the finite blocker register for [#1742](https://github.com/elong0527/yamaa/issues/1742).
It records evidence and decisions still needed, not approval to release or switch
backends. Main `3895017b619642d4cda1d14b0000e20dc65becd7` includes #1811's
combined packages and #1812's complete public cohort reports. Native run
37805429480 passes all seven jobs, all thirteen workflow checks and a clean
full 13-file review, with fresh package/source auditing and independent replay
of all 72 downloaded cohort tuples. Both Unix hosts compare all seventeen
complete original public reports; the 945-row assisted inventory remains
separate. The older matrix below retains historical #1792 evidence. Windows
file transport is the next candidate; cross-target checks and local prototypes
do not settle migration or release acceptance. Follow [the delivery order](migration-order.md).

## Declared API inventory

[release-api.json](release-api.json) records 484 declared names in 34 source files:
363 Python `__all__` entries, 52 native Python stub symbols/members, 22 legacy R
exports, and 47 native R exports/S3 registrations. Re-exports are counted at each
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
| Native Python/R probes, private handles and scalar S3 methods | Migrate qualification consumers, then fold native distributions into `yamaa`. R scalar representation still depends on the INT64_MIN decision. |

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
API test. Windows R, macOS x86_64, Linux arm64, musl, other Python implementations,
R binary distributions and untested versions have no release promise yet. They
are **undecided/unqualified**, not silently approved exclusions.

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
package-local target layout. B15 remains open for a chosen release matrix and
qualification of the final combined public package.
The macOS Rust archive build defaults its deployment target to 11.0 while
preserving an explicitly supplied target. This covers Rust and its C codec
objects; R's own compilation/link settings remain supplied by R. The audited
macOS check has no archive deployment-target warning.
Release scope must either add the appropriate checks or record an explicit
approved exclusion. Package versions in the audited evidence are Python host
0.2.0, native Python/R 0.1.0 and core 0.1.0; none constitutes the final combined
package version policy.

## Finite blocker register

All rows below are current contract or release-decision blockers. No row is
preemptively reclassified as unsupported release scope or a future enhancement.
A disposition that narrows scope requires an explicit linked decision. Issue
numbers identify the implementation/test or decision owner.

| ID | Blocker and current evidence | Owner | Evidence required to close |
| --- | --- | --- | --- |
| B01 | Bounded public frontend and complete Unix public M1 reports qualify in #1811/#1812; Windows file transport remains a candidate. | #1739, #1751 | Both installed public host APIs consume original files, block reference semantics, compare complete observations and saved bytes, and produce evidenced `shared_run` tuples for all six unchanged cases. |
| B02 | Public domain/check/result/log/save API and benchmark runner migration. | #1751 | Installed signatures, wrong-argument exceptions, issue rows for specification/data/environment/function failures, logs retained on failure, explicit save, and all 178 Python/R runner pairs migrated. |
| B03 | API representation, retained helpers and full surface review. | #1751, #1742 | Decide check result, issues.context, full-range R integers/INT64_MIN, custom schema/provenance and IO/ODM/style helpers; test every retained surface and deliberate removal, including class members and undocumented imports. |
| B04 | Declaring-file path policy and production file ports. Existing rules still enforce approved roots/fallback. | #1751, #1755 | Explicit approval for the containment change, reviewed coordinated requirements, path provenance, no fallback, resource preflight, held snapshots and both-host file/error tests. Approval is currently pending after automatic approval review rejected that separate rewrite. |
| B05 | Combined `yamaa` packages qualify in #1811; obsolete probe/duplicate cleanup remains open. | #1751, #1754, #1742 | Clean wheel/source and R package installations exercise combined artifacts; remove duplicate/probe exports only after their replacement consumers qualify. |
| B06 | Environment, lock verification and function-definition migration. | #1757 | Reviewed schema/rules, static validation without code/data, installed-version verification and all called-function tests on every build before study reads; migrate five function fixtures in both languages. |
| B07 | Complete producer workflow. | #1741 | Producer-once execution, function activation before any study data, exact serialized/rounded consumer input, failure ledgers, reuse and explicit publication gates in both installed hosts. |
| B08 | Terminology and submission. | #1757, #1758 | Cross-source uniqueness and bound list verification; new define API and all three submission cases preserve independently expected fixed-time XML/JSON bytes before retiring old formats. |
| B09 | Remaining language/compiler families. Component support is not full-language compilation. | #1585, #1752, #1754 | Qualify handlers, row-output dependencies, predicates/flags/first-available, correlated/keyed aggregates and lookups, full verification scopes, strings/regex/temporal/ODM and submission/value metadata from the executable inventory. |
| B10 | Source/output codecs and filesystem publication. The private original-document route reads CSV/Parquet and writes bounded CSV/Parquet through callbacks; #1785's installed codec audit passed. Actual filesystem publication and producer workflows remain unqualified. | #1755, #1585 | Exact CSV, logical Parquet/ordinal/type contracts, producer re-ingestion, non-regular/missing/changed resources, atomic publication and publication-failure behavior through actual file ports. |
| B11 | Complete diagnostics and failure timing. Core owns preflight, output-declaration, numeric/aggregate grammar, binding/dependency, CSV profile/declared typing, closed Parquet metadata, window/lookup and predicate grammar findings; #1790 qualifies original scalar literals and conversion wrappers; schema kinds are portable after #1789; #1772/#1773 merged captured schema and classified source observations. Remaining diagnostic families, production file-port integration and five assisted-route mismatches remain open. | #1753, #1739 | Preserve normative phase, order, requirement, authored path/context and full source/check/handler ledgers; resolve all five known gaps against independent truth without masking them. |
| B12 | Numerical policy and downstream bytes. #1740 now proposes one behavior per locked release, with no public policy/backend selector; the choice and production policy remain unratified. | #1740 | Ratified one-policy-per-release behavior, implementation in every affected scope, and reviewed disposition for every observable numerical/artifact difference. |
| B13 | Transitive dependency reproducibility. Cargo.lock is ignored and omitted from the staged R source package. | #1742 | Approved no-content-hashing-compatible build process, clean independent resolution/build evidence and retained exact versions/features. Exact direct dependency pins alone do not settle this. |
| B14 | Packaging checksum exceptions. Current AGENTS.md permits only python/uv.lock. | #1757, #1742 | Explicit approval before any additional lock/checksum exception; preserve held-byte resource comparison and prohibit runtime provenance digests. Study locks and Cargo reproducibility are distinct decisions. |
| B15 | Supported platform/version/distribution promises. | #1742 | Ratified matrix, explicit Windows R decision and named exclusions, then complete installed checks including R CMD check for the chosen scope. |
| B16 | Native ownership, interruption, reentrancy and repeated-use behavior across the final facade. Probes already exercise many cases. | #1755, #1742 | Repeat those contracts through final installed domain/check/workflow/save APIs, including failed builds and expired/mismatched handles; probes alone do not close this. |
| B17 | Representative phase/performance/memory evidence. Existing harness is small and cold, with host-assisted preparation. | #1742 | Shared-path workloads at agreed sizes, repeated cold/warm measurements, raw samples/spread, exact truth/callback checks, phase attribution and peak RSS or explicit unavailability. |
| B18 | Performance regression budgets and support tradeoffs. | #1742 | Maintainer-approved numeric budgets before release qualification; evaluate distributions and representative workloads, not one smoke sample or a tiny fixture speedup. |
| B19 | Qualified transition, rollback and evaluator removal. | #1585, #1742 | Explicit transition approval after all supported matrix/API/fixture gates pass; rollback pins a prior release in the host lock; delete duplicate evaluators and update installation/docs/CI deliberately. |

The five timing gaps are named in
[known-gaps.json](../qualification/known-gaps.json): `negative-formula-flag`,
`negative-row-aggregate`, `negative-row-no-prior`, `negative-source-missing-field`
and `negative-source-trivial-filter`. The inspected inventory has 315 reference
passes, 76 assisted native passes, 234 Unsupported results, those five mismatches,
and 315 unexercised R tuples. Three of those original negative documents also pass the private route after
#1791, with independent complete reports and actual ingestion timing; that does
not promote inventory tuples or remove the baseline mismatches. Three additional
predicate requirement/context gaps are documented in `predicate_diagnostics.md`
and remain unqualified. These are baseline route counts, not migration
percentages or evidence that 234 independent compiler features are missing.

## Measurement preparation

Use [the existing phase harness](../PHASE_MEASUREMENTS.md); extend it to the
shared original-YAML frontend after B01/B07. Proposed workloads below need agreed
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
