# yamaa Rust engine migration

Assessed 2026-10-09 against main
`0c52fc6caf761308cd6428822909d7a079a01cad` and the acceptance criteria in
[#1585](https://github.com/elong0527/yamaa/issues/1585).

The goal is to complete one shared Rust implementation behind Python and R,
qualify the supported original-document benchmark and public API matrix, and
retire superseded implementations after their replacements pass. The migration
is active; this assessment does not authorize release cutover. The numerical
choice was subsequently approved on 2026-10-10. The #1757 closing change implements
the public environment path; full release qualification remains separate.
The [2026-10-10 maintainer decisions](maintainer-decisions.md) settle the workflow
lifecycle, submission version override, Windows R requirement, public result
representations, declaring-file paths and Cargo packaging exception. Their
implementation and installed release gates remain open.

## Delivered and qualified

- Shared schema capture, original-YAML preparation, inheritance, compilation,
  source inspection/capture, CSV/Parquet codecs, execution, diagnostics, owned
  results and explicit native publication are implemented. Both hosts install
  one package named `yamaa`.
- The bounded six-document public M1 cohort passes 96 installed tuples across
  direct/source Python forms and the exercised R platforms. Twelve Windows R
  tuples remain explicitly unqualified under #1742. The separate 954-row
  benchmark inventory is still reference-assisted.
- Owned environment admission, versionless column and row calls, shared
  lock/bind/all-cases activation and governed column codelist checkpoints have
  merged. Compilation and activation failures precede study-data authority;
  original source bytes, handler/check observations and opaque host failures
  remain retained.
- [#1835](https://github.com/elong0527/yamaa/pull/1835) supplies bounded installed
  package-version verification and ordinary exact-signature resolution in both
  hosts. All 47 Python helper regressions, six installed Python package-form
  witnesses and both installed R source witnesses pass.
- [#1837](https://github.com/elong0527/yamaa/pull/1837) retains each build's
  actually attempted activation stages and earlier successful test results
  through later failures, interrupts and unwind. Its qualified shared source
  passes 953 Rust tests across 120 targets. Actual Windows debug/release-test
  logs include all 32 affected activation/project tests.

- [#1844](https://github.com/elong0527/yamaa/pull/1844) connects owned project
  preparation to native approved resources and preserves original lock/metadata
  through each typed build. Its final source passes 968 shared-crate tests across
  121 targets, both installed Python forms and all 17 strict R scripts. Actual
  Windows debug/release-test logs include all 15 new project/lock-fact tests.

- [#1848](https://github.com/elong0527/yamaa/pull/1848) connects actual installed
  Python/R host capabilities to retained native project attempts. The final
  source passes 969 shared-crate tests across 121 targets, both fresh installed
  Python forms and all 17 strict R scripts. Its project witness passes in all
  six CI Python forms and both exercised R platforms. Actual Windows
  debug/release-test evidence includes all 12 affected project/file-project cases.

These PRs passed their complete reviews, every applicable workflow check,
all 16 native CI jobs and all 37 downloaded artifact/source audits. Their
merged source trees equal the qualified heads directly. These results establish
the stated slices; component counts are not a migration percentage.

## Current implementation frontier

The installed Python/R connection is qualified through private native project
handles with explicit candidate schemas. These call the shared whole
project build, retain typed attempts and prepared documents independently, and
project bounded lossless activation evidence without repeating host effects.
The shared Rust project report formatter now borrows a whole retained attempt,
including classified activation failures, source reads, codelist values and
completed-check prefixes. It uses the ordinary output/save gate and matches
independent complete truth for sixteen original benchmark documents. Private
Python/R result carriers now retain the original whole attempt beside a complete
report. Installed witnesses cover exact output/save bytes, failed-save gates,
ordinary failures and original interrupts after the first attempt handle is
collected. Report quota refusal keeps all 360 original failures; Python error
rendering also refuses reentrant mutable access without deadlocking.
The #1757 closing change now connects these retained results to public
`domain(..., environment=...)` and `check(..., environment=...)` in Python and R.
The shipped environment/function schemas and rules use ordinary installed code,
versionless calls, uv/renv locks and inline function tests. Static checking grants
no code or study-data authority; each build freshly verifies called packages and
runs all called-function cases before study reads. Environment codelists compile
and run at the existing column verification boundary.

All five function benchmarks have equivalent Python/R environments. Four retain
their exact original CSV truth; the fifth independently specifies a missing
required argument (REQ-0700), replacing the removed contract-version error.
Installed shared public suites replace the three obsolete function-specific
reference-assisted qualification rows. The wider inventory continues to expose
unsupported reference routes; it does not count them as shared execution.
The obsolete artifact resolvers, contract fingerprints, cached activation and
vendored benchmark code are removed. The old submission schema/generator and
three Define-XML fixtures remain for #1758. The #1905 declaring-file candidate
removes root-configuration discovery and fallback from replacement public routes.
Relative paths retain each declaring layer; absolute and parent-relative local
paths use the same native kind, link and held-byte snapshot checks in both hosts.
Private explicit-root probes retain historical qualification behavior. Final-head
review, installed platform evidence and remaining release gates are separate.

The #1858 producer compiler prerequisite adds a separate sealed metadata boundary
with retained producer identities and authored layer paths. Ordered output
contracts, required labels, redundant inline types and artifact metadata
comparison now have shared portable causes. Ordinary check/build routes still
reject producer schemas, and the metadata result explicitly refuses an execution
capability. Its 25 independent contract/compiler/engine/adapter tests establish
admission only. Native graph preparation, producer execution and serialized
rounded consumer ingestion remain #1741. [#1859](https://github.com/elong0527/yamaa/pull/1859)
merged as `ffe4a5a2da53596d2c54207f03efa08635545418` and closed #1858 after all
22 final-head checks, 16 native jobs, completed review and 40 artifact audits passed.
The merged source equals its qualified head. See [the boundary contract](../PRODUCER_ADMISSION.md)
and [the qualification record](1858-qualification.md).

The #1860 output prerequisite now admits shared CSV `output.decimals` and formats
the exact binary64 fraction with genuine ties away from zero. Precision affects
serialized bytes only; retained tables and same-node calculations stay unrounded.
Portable invalid/Parquet diagnostics, arbitrary-width metadata quotas and explicit
failed-save gates retain their existing lifecycle. See [the precision contract](../CSV_PRECISION.md)
and [its qualification record](1860-qualification.md). [#1862](https://github.com/elong0527/yamaa/pull/1862)
merged as `0267431afbe1d21f5806d01f21b25d711d69fe23` and closed #1860 after
all 23 final-head checks, 16 native jobs, completed review and 40 artifact audits
passed. Its merged source equals the qualified head. This slice does not supply
producer execution or consumer re-ingestion.

The [#1863 complete producer graph metadata](https://github.com/elong0527/yamaa/issues/1863)
prerequisite is qualified: one canonical document per node,
complete acyclic closure, deterministic producer-first order, one compilation per
node and explicit local-to-graph function slots against one owned environment.
Cross-branch schema/snapshot and producer metadata conflicts refuse. This capability
still exposes metadata and execution denial only; it grants no native resources,
activation, producer execution, ingestion or publication authority. See
[the boundary](../PRODUCER_GRAPH.md) and [qualification record](1863-qualification.md).
[#1865](https://github.com/elong0527/yamaa/pull/1865) merged as
`4bfa2d8300539b598f71cb26991914301cd088a1` after all 23 final-head checks,
16 native jobs, complete assessed review and 40 artifact audits passed. Its
merged source equals the qualified head directly. Linux/macOS pass 1,037 Rust
tests per mode; Windows passes 1,028, across 125 targets each. Six installed Python
forms and both exercised R forms pass their complete supplemental suites.

The [#1867 producer graph capture](https://github.com/elong0527/yamaa/issues/1867)
prerequisite captures each original recursive producer document once under
approved descriptor roots, retains independent authored origins and rejection
evidence, and supplies artifact location facts while prospective outputs remain
absent. It captures a canonical closure iteratively through shared
source/inheritance preparation, verifies aliases and whole-prefix snapshots,
bounds capture/copies/text/work, and retains original raw failure evidence.
Component filesystem tests cover diamonds, separate inherited origins, absent
artifacts, mismatch/cycle/missing/binding failures, limits, fallback, links,
changed snapshots and literal-backslash Unix roots.
[#1868](https://github.com/elong0527/yamaa/pull/1868) merged as
`0beaecaae144d12a536c988e8b336cd0323a0add` after all 22 final-head checks,
16 native jobs, a complete assessed review, 40 artifact audits and a fresh guard
passed. Its merged source equals qualified head
`75f5fabd9b60175bb17b5527e407bd7820a608d1` directly. Actual Rust debug/release-test
passes 1,056 tests on Linux/macOS and 1,044 on Windows across 126 targets each.
This boundary grants no workflow execution authority. See
[the qualification scope](1867-qualification.md).

The [#1869 shared producer build orchestration](https://github.com/elong0527/yamaa/issues/1869)
prerequisite is qualified by [#1870](https://github.com/elong0527/yamaa/pull/1870),
merged as `2214ea79d952913285421f065307bbc52ba80e71`. All 22 final-head checks,
16 native jobs, complete assessed 24-file review, 40 artifact audits and a fresh
protected guard passed. Its merged source equals qualified head
`2638559359bd2d0bf798100f0e9b206104362c04` directly.
Its separate whole-graph owner now activates and validates all local/global full
call signatures before study data, executes canonical producers once, and admits
accepted serialized artifact re-ingestion through ordinary verification/output/
codec/report gates. Whole entered attempts retain original failures, interrupts,
unwind and source/check prefixes. Native witnesses preserve inherited origins,
metadata/study snapshot ordering, exact rounded CSV truth, Parquet cells and a
shared diamond. Actual Linux/macOS debug/release modes pass 1,079 tests and Windows
passes 1,066 across 128 targets each. Strict five-crate Clippy, all 48 tool guards,
clean installed forms and package/evidence audits pass. See [the service](../PRODUCER_BUILD.md) and
[qualification scope](1869-qualification.md). Installed workflow dispatch and
ordered publication remain #1741. The approved #1866 lifecycle keeps execution
in memory and stops dependent publication on Save failure.

The [#1871 complete retained producer graph reports](https://github.com/elong0527/yamaa/issues/1871)
prerequisite is qualified by [#1873](https://github.com/elong0527/yamaa/pull/1873),
merged as `0254075111c18eb41f13ab8d34c094660541abf7`. All 22 final-head checks,
16 native jobs, complete assessed 17-file review, 40 artifact audits and a fresh
protected guard passed. Its merged source equals qualified head
`924d0a63933973d337372cf0aa6e545815e32f25` directly.
Its implementation borrows coherent original document/compiled-plan contexts from
the sealed whole graph, gates dependent ingestion on complete native node reports,
and retains whole union activation and actual entered-node failure evidence.
Count/text/cell/work admission precedes report copying. Independent complete
rounded CSV truth, native Parquet, source/check/opaque failure, repeat/drop-owner,
late quota and contradictory-evidence witnesses pass. Actual Linux/macOS
debug/release modes pass 1,088 tests and Windows passes 1,075 across 128 targets
each. Clean installed forms and all 14 package audits pass, including 4,511 direct
byte comparisons. Ordinary installed workflow and
publication acceptance remains #1741. See [the reporting contract](../PRODUCER_REPORTS.md)
and [qualification scope](1871-qualification.md).

The [#1874 ordinary producer graph preparation/checks](https://github.com/elong0527/yamaa/issues/1874)
prerequisite is qualified by [#1876](https://github.com/elong0527/yamaa/pull/1876),
merged as `01fdd553212605f7147e017212d074470214cb5b`. All 23 final-head checks,
16 native jobs, complete assessed 17-file review, 40 artifact audits and a fresh
protected guard passed. Its merged source equals qualified head
`330638f3b8067e89aa0a22a3bfe08aa1adf9b8cf` directly. Ordinary installed Python/R
checks now capture one environment and the complete original recursive graph,
retain cumulative quotas and rejection evidence, and project complete bounded
static findings with actual inherited origins. Checking grants no activation,
study-data or publication authority. Actual Linux/macOS modes pass 1,101 Rust
tests and Windows passes 1,087 across 129 targets each. Fresh installed Python
forms pass all 24 suites; exercised R forms pass all 18 with strict package
checks. See the [static contract](../PRODUCER_CHECKS.md) and
[qualification scope](1874-qualification.md).

The [#1877 installed producer build handles and retained reports](https://github.com/elong0527/yamaa/issues/1877)
prerequisite is qualified by [#1880](https://github.com/elong0527/yamaa/pull/1880),
merged as `3a919d02d03766281e8dbd280a1ca69fe35a38c2`. All 23 final-head checks,
16 native jobs, complete assessed 23-file review with no actionable findings,
40 artifact audits and a fresh protected guard pass. Its merged source directly
equals qualified head `8a7694169c5f5510a88f53f9dc858f1257bf2ad7`.
Private complete Python/R carriers retain original attempts, raw sources and
host failures through bounded classified reporting, reentrancy and owner/file
deletion. Native report gates precede accepted serialized producer consumption.
Actual Linux/macOS modes pass 1,106 Rust cases and Windows passes 1,092 across
129 targets each. All fresh installed Python forms pass 25 suites; exercised R
forms pass 19 and strict Status OK. All 14 packages pass 4,547 direct source-byte
comparisons; 29 evidence rows and 30,033 PortableLibmV1 samples pass. See the
[private host carrier contract](../PRODUCER_HOST_BUILDS.md) and
[qualification scope](1877-qualification.md).

The [#1881 original workflow selection](https://github.com/elong0527/yamaa/issues/1881)
prerequisite is qualified by [#1882](https://github.com/elong0527/yamaa/pull/1882),
merged as `1eb38974dc8b1cfb2936c3b7bcb8114555fba418`. Its merged source directly
equals qualified head `51044204e0fc4a5d5bab8e7fbee04bdf25b68ab3`. All 22 applicable
final-head contexts, all 16 native jobs, complete assessed 15-file review,
40 artifact audits and a fresh protected guard pass. Native selection consumes
one sealed original capture into the existing ordinary FileProject or complete
FileGraph. Inherited non-null producer declarations select structurally before
compilation; graph collection receives the original root directly. Known root
and schema admission precedes the first environment read, cumulative metadata
quotas and raw rejection owners stay retained, and ordinary scope transition,
reports, activation and explicit held-byte Save remain compatible. Actual
Linux/macOS modes pass 1,122 Rust cases and Windows passes 1,108 across 129 targets.
Fresh installed Python forms pass 25 suites; exercised R forms pass 19 and strict
Status OK. All 14 packages pass 4,547 direct source-byte comparisons, with complete
29-row evidence and 30,033 PortableLibmV1 sampled observations. See the
[native selection contract](../WORKFLOW_SELECTION.md) and
[qualification record](1881-qualification.md).

The [#1883 installed producer domain build results](https://github.com/elong0527/yamaa/issues/1883)
prerequisite is qualified by [#1885](https://github.com/elong0527/yamaa/pull/1885),
merged as `421dc17f4e24bf48d8589dd98e3a9c3ac1690f73`. Its merged source directly
matches qualified head `68746afe882c769723375c66530ad055d570fad1`. All 23 applicable
contexts, 16 native jobs, complete assessed 22-file review, 40 artifact audits and
a fresh protected guard pass. Public explicit-environment producer entries retain
complete classified whole attempts and bounded unrounded root output; consumers
use accepted rounded serialized bytes. Actual modes pass 1,127 Rust cases on
Linux/macOS and 1,113 on Windows across 129 targets. Six installed Python forms
pass 26 suites each; exercised R forms pass 20 and strict Status OK. All 14 packages
pass 4,554 direct byte comparisons, with 29 evidence rows and 30,033 PortableLibmV1
sample observations. See [the result contract](../PRODUCER_PUBLIC_RESULTS.md) and
[qualification record](1883-qualification.md). Producer publication remains
#1741 under the approved #1866 lifecycle; this slice supplies no graph publisher
or Save capability.

The [#1888 bounded typed submission declarations](https://github.com/elong0527/yamaa/issues/1888)
prerequisite is qualified by [#1891](https://github.com/elong0527/yamaa/pull/1891),
merged as `854b6bf4de3723654b413d98fa2464a26fab43f9`. Its merged source directly
equals qualified head `28af585550b8bafade330d028406c5f654dcb330`. All 22 applicable
contexts, 16 native jobs, complete assessed ten-file review, 40 artifact audits
and a fresh protected guard pass. Actual modes pass 1,143 Rust cases on Linux/macOS
and 1,129 on Windows across 130 targets. Six Python forms pass 26 suites each;
both exercised R hosts pass 20 and strict Status OK. All 14 packages pass 4,574
direct byte comparisons, with 29 evidence rows and 30,033 PortableLibmV1 samples.
Core declaration decoding now retains a complete closed owned study/section
model, authored section/specification/document order, existing defaults and
overrides, and original normalized field occurrence facts. Whole count, text,
escaping, depth, work and finding capacities are admitted before ownership;
refusals preserve the exact borrowed input without a fitting diagnostic prefix.
Sixteen independently authored cases cover all families, scalar/shape failures,
limits and retention after the original document drops. See
[the declaration contract](../SUBMISSION_DECLARATIONS.md) and
[qualification record](1888-qualification.md). This metadata model provides no
execution, rendering, artifact-read or publication authority.

[#1893 specification submission declarations](https://github.com/elong0527/yamaa/issues/1893)
is closed by [#1896](https://github.com/elong0527/yamaa/pull/1896), merged through
GitHub as `8b369f1a96359f5454ab5669b476308be80cdad3` while this chat prepared a
rebase after main advanced. The final merge guard was not executed by this chat.
All nine authored paths equal the previously qualified head, but the whole merged
tree differs through intervening main changes. Its old-base qualification and
precise limits are recorded in [the evidence ledger](1893-qualification.md).
Its closed metadata owner retains dataset and ordered columns, existing defaults,
plain/structured alternatives, document/page references, arbitrary-width integer
declarations and normalized occurrence facts. Complete non-owning resource and
structural admission precedes model or finding growth. Independent core cases
cover every current vocabulary, order/default/null/shape distinctions, cumulative
limits and retention after input deletion. See
[the boundary](../SPECIFICATION_SUBMISSION_DECLARATIONS.md).
[#1897 document-list rejection occurrence facts](https://github.com/elong0527/yamaa/issues/1897)
is closed by [#1901](https://github.com/elong0527/yamaa/pull/1901), merged through
GitHub at `48eb58c30e3cb9ca587b6c237a77a63977b644ef`. Malformed environment
document items now identify their actual sequence container, absent mapping key
and item node alongside the authored section/index path. Independent cases cover
all three families, multiple positions, complete quota refusals and unaffected
valid ownership after input deletion. The originating chat did not execute its
final merge guard: fresh 0.3.0 strict R qualification failed because five benchmark
SDK locks still pinned 0.2.0. PR #1904 corrected those pins before #1901 merged.
The original failure is scoped to its preceding base; earlier passing 0.2.0
evidence is not current-release proof. See
[the merge observation](https://github.com/elong0527/yamaa/pull/1901#issuecomment-6102664991).

The active [#1907 SDK lock coherence correction](https://github.com/elong0527/yamaa/issues/1907)
retains #1904's existing SDK corrections and adds an early repository guard plus
an actual installed mismatch refusal witness through
[#1909](https://github.com/elong0527/yamaa/pull/1909). Program versions and
independent truth remain unchanged. See
[the correction contract](../BENCHMARK_SDK_LOCKS.md). Local qualification of
runtime source `6e79094c2352cfccba60ef263a714ac8db09e090` passed both installed
Python forms and strict installed R, including the SDK mismatch witness. The
subsequent merge of #1908 changes documentation only; runtime source and fixtures
remain identical. Complete review, applicable current-head checks, installed
artifact audits and a fresh protected merge guard remain required.
Semantic composition, original inherited capture and generators remain #1758.
The approved [#1887 decision](https://github.com/elong0527/yamaa/issues/1887)
adds an optional bounded nonempty section-level `source_system_version` override,
defaulting to the installed yamaa version. Migrated fixtures explicitly declare
`"1.0"` to preserve committed XML/JSON bytes; qualification records the actual
generator version separately. Implementation remains #1758.

See [the closing qualification record](1757-qualification.md) for actual local
results and the distinction between those results and the CI platform matrix.
Continue with the following acceptance gates in the
[serial implementation order](migration-order.md):

1. #1757 is closed by [#1856](https://github.com/elong0527/yamaa/pull/1856),
   merged as `aff53fb65df5b9d6a45d8cf203fa0b9b52f1176a`. All 26 final-head
   checks, all 16 native jobs, completed review and 40 artifact audits pass.
   The merged source equals the qualified head. Retain #1753's observation
   contracts and #1755's port ordering. #1858's producer compiler prerequisite
   is closed by #1859 with its complete qualification above.
2. [#1860 CSV precision](https://github.com/elong0527/yamaa/issues/1860) is closed
   by #1862 with complete qualification above.
3. #1863 is closed by #1865, #1867 by #1868, #1869 by #1870 and #1871 by #1873
   with complete qualification above. #1874 is closed by #1876, #1877 by #1880,
   #1881 by #1882 and #1883 by #1885. Complete #1741's
   producer workflow: compile the graph once, activate before
   data, execute producers once, serialize rounded outputs before consumer
   ingestion, retain failures and publish only through explicit successful save.
   Implement #1866's approved lifecycle: publication failure stops dependent
   publication while retaining completed execution evidence and prior writes.
4. #1888 is closed by #1891 and #1893 by #1896 with its evidence limits above;
   #1897 is closed by #1901. Local qualification covers #1907's SDK lock
   coherence correction and installed R as scoped above. Complete its review,
   applicable current-head checks, artifact audits and protected merge guard,
   then complete #1758's shared Define-XML/Dataset-JSON implementation, approved
   source-system-version override and three submission fixtures before retiring
   the old schema and generator.
5. Close the remaining language/API gaps in #1751/#1752/#1754 and the executable
   inventory, migrate every supported benchmark runner, and qualify complete
   output, issues, logs and exact saved bytes through both installed public APIs.
6. Complete #1740/#1742's numerical, reproducibility, platform and representative
   performance gates, including required Windows R (#1903), approved resource
   paths (#1905) and locked Cargo builds (#1906). Remove duplicate evaluators and
   superseded APIs only after the approved complete supported matrix passes.

## Approved decisions and remaining qualification

The [maintainer decision record](maintainer-decisions.md) records eight choices
approved on 2026-10-10. #1866, #1887 and #1857 are closed as decisions; their
workflow, submission and Windows R implementation gates remain #1741, #1758 and
#1903. #1751 records issues-only checks, lossless full-range R integer vectors and
JSON-text diagnostic context. The declaring-file path replacement is approved
and tracked in #1905; current resource-root behavior remains until that change
qualifies.

The Cargo packaging exception in [AGENTS.md](../../AGENTS.md) permits one canonical
`rust/Cargo.lock` and unchanged staged Python/R source-package copies. Commit the
lock, enforce locked builds and obtain clean installed evidence under #1906.
Runtime identity, contract, activation-cache and resource-snapshot digests remain
prohibited. Existing Python UV locks and hash-free R study locks remain usable.

The previously approved pinned `PortableLibmV1` policy and its accepted historical
Python differences remain in force; see [the numerical decision](1757-maintainer-decisions.md)
and [math policy](../MATH_POLICY.md). Full language/API qualification, retained
helper dispositions, the remaining version/architecture/distribution matrix,
representative measurements and numeric performance budgets remain release gates.
Keep old submission and reference assessment paths until their replacements
qualify. These decisions do not authorize default cutover or close #1585.
