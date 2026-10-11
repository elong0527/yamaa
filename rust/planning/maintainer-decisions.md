# Rust migration maintainer decisions

The maintainer approved the first eight choices on 2026-10-10 in the Rust
migration assessment conversation, then approved the complete first-release
platform matrix and its exclusions in the implementation conversation. They set
the implementation targets for the first shared Rust release. Implementation,
installed qualification and release transition remain subject to their
acceptance gates.

| Decision | Approved behavior | Implementation and qualification |
| --- | --- | --- |
| Producer lifecycle | Build the graph in memory; explicit Save publishes retained bytes in producer-first order. | [#1741](https://github.com/elong0527/yamaa/issues/1741); decision [#1866](https://github.com/elong0527/yamaa/issues/1866). |
| Submission version | Optional section-level `source_system_version`, defaulting to the actual installed yamaa version. | [#1758](https://github.com/elong0527/yamaa/issues/1758); decision [#1887](https://github.com/elong0527/yamaa/issues/1887). |
| Windows R | Required in the first shared Rust release. | [#1903](https://github.com/elong0527/yamaa/issues/1903), [#1742](https://github.com/elong0527/yamaa/issues/1742); decision [#1857](https://github.com/elong0527/yamaa/issues/1857). |
| First-release support matrix | CPython 3.12-3.14 and R 4.6.1 on Linux x86_64, macOS arm64 and Windows x64; Python wheel/source and R source packages. Other combinations are excluded from this first release. | [#1742](https://github.com/elong0527/yamaa/issues/1742); installed Windows R [#1903](https://github.com/elong0527/yamaa/issues/1903). |
| Static check result | Diagnostic issues only. | [#1751](https://github.com/elong0527/yamaa/issues/1751). |
| R integer output | A lossless full-range `yamaa_int64_vector`, with missingness separate from every signed 64-bit value. | [#1751](https://github.com/elong0527/yamaa/issues/1751). |
| Diagnostic context | JSON text in both Python and R. | [#1751](https://github.com/elong0527/yamaa/issues/1751). |
| Resource paths | Resolve relative declarations from their original declaring file; permit absolute and parent-relative local paths without yamaa root containment or fallback. | [#1905](https://github.com/elong0527/yamaa/issues/1905), [#1751](https://github.com/elong0527/yamaa/issues/1751). |
| Rust dependency locking | One canonical `rust/Cargo.lock`, unchanged source-package copies and enforced locked builds, with a packaging-checksum exception. | [#1906](https://github.com/elong0527/yamaa/issues/1906), [#1742](https://github.com/elong0527/yamaa/issues/1742). |

## Producer lifecycle

`domain()` computes the graph without publishing files. Root output is available
before Save. Consumers ingest accepted serialized producer bytes, including
declared rounding, while same-node calculations and retained tables remain
unrounded.

Explicit Save publishes the already retained producer bytes before their
consumers. It does not reread sources, rerun callbacks or encode the output again.
Execution, checking or rendering failure stops dependent execution. A publication
failure stops dependent publication, retains completed execution evidence and the
original failure, and leaves already published files in place. Publication is
not a rollback transaction over earlier successful writes.

## Submission source system version

An optional bounded nonempty `source_system_version` field on the submission
section overrides the generated source-system version in both Define-XML and
Dataset-JSON. Omitting it uses the actual installed yamaa package version.

The migrated fixtures explicitly declare `"1.0"` to preserve their independent
committed expected bytes. Qualification records the actual generator package
version separately. Keep the old submission generator and format until the
replacement and all three migrated fixtures qualify.

## Windows R release scope

Windows R is a required first-release platform alongside Linux/macOS R and the
existing Python Windows scope. Complete native Windows R file ports, source
package build/install, public and cohort suites, and strict `R CMD check` before
cutover. The twelve currently missing Windows R cohort tuples remain unqualified.
The approved complete target and explicit exclusions are recorded below.

## First-release support matrix

The maintainer selected "Use this matrix and explicitly exclude other
combinations for the first release" in response to the implementation
conversation's support-scope question. This resolves the B15 scope decision;
installed qualification remains required.

| Runtime | Required platforms | Required distribution forms |
| --- | --- | --- |
| CPython 3.12, 3.13 and 3.14 | Linux x86_64, macOS arm64 and Windows x64 | Wheel and independent source-package rebuild on every interpreter/platform pair. |
| R 4.6.1 | Linux x86_64, macOS arm64 and Windows x64 | Source-package build/install, full installed suites and strict `R CMD check`. |

Other runtime versions or Python implementations, free-threaded Python, other
architectures (including macOS x86_64, Linux arm64 and Windows arm64), musl/Alpine
and R binary distributions are explicitly excluded from this first release.
This decision does not remove language or API requirements within the approved
matrix. Future releases may add combinations with a separate decision and
installed evidence.

The qualification environments are Ubuntu 24.04/glibc, macOS 15/arm64 and the
Windows x64 Actions image. Record their exact runtime/image versions in each
release audit; the target does not promise additional OS baselines or binary
compatibility merely because a package manifest accepts them. The abi3-py312
wheel is built once per platform and must pass installed qualification on all
three interpreter versions. Source packages must independently build and run on
each interpreter. Qualification of one tuple never substitutes for another.

## Public result representations

The first release's `check()` result contains diagnostic issues only. It does
not return resolved YAML, a resolved specification or a compiled plan.

R integer output uses `yamaa_int64_vector` and preserves the full signed 64-bit
range. `INT64_MIN` is a real value; missingness is represented separately.
Supported methods, subsetting, conversions and data-frame ownership must qualify
through the installed hosts, including Windows R. Conversions are exact or fail
explicitly; they must not silently narrow to an R integer or a binary64 double.

`issues.context` is a JSON-text string column in both hosts, produced by the
shared encoder. Apply the representation consistently to public issue tables,
including submission diagnostics as they are implemented. Host adapters do not
convert context into Python dictionaries or R list columns.

## Declaring file resource paths

Resolve each relative declared resource path from the directory of the original
YAML file or layer that declared it. A missing file fails at that location.
Remove fallback to another project/root location, project-configuration discovery
for path search, and yamaa's approved project/data-root containment requirement.
Absolute local paths and `../` paths are allowed subject to operating-system
permissions and the caller's sandbox.

Preserve existing file-kind, symbolic-link and snapshot-consistency checks unless
separately changed. This decision does not add URI or network resource support.
Review the coordinated normative requirements and both-host file/publication
tests under #1905 before treating the replacement as qualified.

## Cargo dependency locking

Commit one canonical `rust/Cargo.lock` and stage unchanged copies into the Python
and R source packages. All supported build paths must enforce locked dependency
resolution. Retain the exact resolved transitive versions, features and toolchain
with clean direct/source Python and R package-build evidence on every required
platform, including Windows R.

The [repository convention](../../AGENTS.md) permits packaging-tool checksum
fields in `python/uv.lock`, the canonical `rust/Cargo.lock` and unchanged staged
copies of that Cargo lock. This supersedes the earlier Cargo-exception decision
in [1757-maintainer-decisions.md](1757-maintainer-decisions.md). It does not permit
runtime identity digests, hashed contract fingerprints, activation-cache digests
or resource-snapshot digests. Hash-free R study locks remain the existing path.
Committing and enforcing the lock is implementation work under #1906.

## Completion decisions for the work-package migration

On 2026-10-11 the maintainer instructed the migration implementation agent to
complete the open work packages and self-approve decisions without further
confirmation. The following decisions resolve the initial questions in
[#1973](https://github.com/elong0527/yamaa/issues/1973). They are decisions about
the target behavior, not claims that its implementation or qualification passes.
The work-package umbrella is
[#1969](https://github.com/elong0527/yamaa/issues/1969); the superseded issues
retain historical evidence.

| Item | Approved disposition | Implementation owner |
| --- | --- | --- |
| 1 | Retain Python domain/check/define and their error/result APIs, R equivalents, version metadata, and Python read_odm/write_odm_parquet. Remove public IO and resolved-spec/provenance extensions, the non-reader ODM implementation, and cdiscbuilder at qualified cutover. Keep the style CLI with private implementation, ported away from reference imports onto packaged YAML/schema services. | #1991, #1993, #1994, #2002 |
| 2 | Export and qualify R yamaa_define. | #1991 |
| 3 | Preserve the lossless yamaa_utf8_vector for columns containing NUL, separate missingness, exact subsetting, and preserve-or-error conversions. Ordinary text columns remain character. This preserves the previously approved representation recorded in PR #2003. | #1991 |
| 4 | Python environment remains keyword-only; wrong argument types raise TypeError. R uses a classed yamaa_argument_error inheriting error. A missing specification remains an issue row. | #1991 |
| 5 | No JSON request/reply protocol crosses a host callback boundary. Retire inheritance/1 and the function/1 and dataset/1 probes after replacement consumers qualify. JSON-text issue context remains diagnostic data. | #1988, #1990 |
| 6 | REQ-0191 governs invalid ESCAPE literals. Reconcile REQ-0192 with parse-time invalid_predicate and temporal details in context; do not replace committed benchmark truth. | #1983 |
| 7 | Assign normative requirements to the six language causes currently lacking them. Only unsupported_operation and engine_rejected retain explicitly documented null requirements as implementation findings. Register every emitted condition and its phases. | #1984, #2004 |
| 8 | Native, R glue, packaging, and native CI changes require a green full matrix before merge, including Windows R. #1972 owns the planning narrative; existing per-slice qualification ledgers are frozen. | #1971, #1972 |
| 9 | Freeze assisted inventory additions now. Retire the assisted inventory once the public conformance ratchet passes the full matrix, before deleting its probe consumers; preserve history in Git. | #1974, #1990, #1994 |
| 12 | Correct schema-inheritance-project-root's declared resource to ../input/dm.csv and its explanation, preserving independent expected output. | #1974, #1995 |
| 13 | Distribute the staged R source tarball as a GitHub release asset for the first release. Document source installation and the required Rust toolchain; r-universe and CRAN remain later distribution work. | #1993, #1999 |
| 14 | Keep the four expected/spec_resolved.yaml files as reference-only history, explicitly exclude them from public conformance, and remove them at qualified cutover. No resolved-spec public producer is retained. | #1974, #1994 |
| 15 | Codify cut's reference payload checks: strictly ascending finite binary64 breaks and one label per interval. Validate before study reads. | #1975 |
| 16 | Register empty_string_present_unsupported under REQ-1161 in the validation phase for CSV empty_string: present. | #1976 |
| 17 | Check greatest/least comparability statically from declared types under REQ-0324. Equal int/float values retain the first source's type; document this existing behavior in the expression rules. | #1978 |
| 18 | An integer predicate literal outside signed 64-bit reports integer_overflow during parsing. Do not widen the language's integer type. | #1983 |

The numerical performance budgets (item 10) remain to be selected from #1998's
actual comparable measurements. Preserve the approved lookup, aggregate, window,
callback and producer scope at 1,000/10,000/100,000 rows, ten cold samples and
thirty warm samples after three retained warm-ups. Measure the reference before
removing it and compare the previous release on the same machine. Capacity
refusals and unavailable memory metrics are findings, not successful samples.

The implementation agent may approve the transition (item 11) after #1999's
concrete plan and all supported language, API, installed-matrix and performance
gates pass on the combined main revision. Retain the reference until that gate;
rollback pins the previous release. These pending evidence decisions do not
require additional maintainer confirmation, and are not preemptively approved
by this record.

## Remaining release gates

The approved numerical policy remains pinned `PortableLibmV1`, one behavior per
locked yamaa release, with the accepted historical Python differences described
in [the numerical decision](1757-maintainer-decisions.md) and
[math policy](../MATH_POLICY.md).

The [release blocker register](release-readiness.md) still requires remaining
language families, complete public API and retained-helper dispositions, native
resource/failure behavior, benchmark migration, the supported installed matrix,
clean reproducible builds and representative performance/memory measurements.
Numeric regression budgets follow those measurements. These approvals do not
authorize narrowing the supported language or performing release cutover.
Track complete qualification and the explicit transition gate in
[#1969](https://github.com/elong0527/yamaa/issues/1969). The superseded
[#1585](https://github.com/elong0527/yamaa/issues/1585) is historical evidence.
