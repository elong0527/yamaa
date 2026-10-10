# Rust migration maintainer decisions

The maintainer approved these eight choices on 2026-10-10 in the Rust migration
assessment conversation. They set the implementation targets for the first shared
Rust release. Implementation, installed qualification and release transition
remain subject to their acceptance gates.

| Decision | Approved behavior | Implementation and qualification |
| --- | --- | --- |
| Producer lifecycle | Build the graph in memory; explicit Save publishes retained bytes in producer-first order. | [#1741](https://github.com/elong0527/yamaa/issues/1741); decision [#1866](https://github.com/elong0527/yamaa/issues/1866). |
| Submission version | Optional section-level `source_system_version`, defaulting to the actual installed yamaa version. | [#1758](https://github.com/elong0527/yamaa/issues/1758); decision [#1887](https://github.com/elong0527/yamaa/issues/1887). |
| Windows R | Required in the first shared Rust release. | [#1903](https://github.com/elong0527/yamaa/issues/1903), [#1742](https://github.com/elong0527/yamaa/issues/1742); decision [#1857](https://github.com/elong0527/yamaa/issues/1857). |
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
Other version, architecture and distribution promises still require a supported
matrix and installed evidence under #1742.

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
Keep [#1585](https://github.com/elong0527/yamaa/issues/1585) open through complete
qualification and the explicit transition gate.
