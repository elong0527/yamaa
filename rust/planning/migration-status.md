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
three Define-XML fixtures remain for #1758. Approved resource-root configuration
also remains until its own path-policy decision.

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

The next #1741 prerequisite is [#1863 complete producer graph metadata](https://github.com/elong0527/yamaa/issues/1863).
Its bounded implementation is being qualified: one canonical document per node,
complete acyclic closure, deterministic producer-first order, one compilation per
node and explicit local-to-graph function slots against one owned environment.
Cross-branch schema/snapshot and producer metadata conflicts refuse. This capability
still exposes metadata and execution denial only; it grants no native resources,
activation, producer execution, ingestion or publication authority. See
[the boundary](../PRODUCER_GRAPH.md) and [qualification scope](1863-qualification.md).

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
3. Qualify #1863's complete static graph prerequisite, then complete #1741's
   producer workflow: compile the graph once, activate before
   data, execute producers once, serialize rounded outputs before consumer
   ingestion, retain failures and publish only through explicit successful save.
4. Complete #1758's shared Define-XML/Dataset-JSON implementation and three
   submission fixtures before retiring the old schema and generator.
5. Close the remaining language/API gaps in #1751/#1752/#1754 and the executable
   inventory, migrate every supported benchmark runner, and qualify complete
   output, issues, logs and exact saved bytes through both installed public APIs.
6. Complete #1740/#1742's numerical, reproducibility, platform and representative
   performance gates. Remove duplicate evaluators and superseded APIs only after
   the approved complete supported matrix passes.

## Decisions still separate from implementation

The pending declaring-file-only path policy has not been adopted: current
approved roots, configuration and fallback behavior remain. The maintainer
approved pinned `PortableLibmV1` for implementation and qualification on
2026-10-10, accepting the documented historical Python differences and their
downstream effects. See the [decision record](1757-maintainer-decisions.md) and
[math policy](../MATH_POLICY.md). Release representations, Windows R scope, Cargo
reproducibility and performance budgets retain their decisions and qualification
gates; numerical approval does not close them.

AGENTS.md still permits tool hashes only in the existing `python/uv.lock`.
Additional digest-bearing lock files are not authorized. The verified existing
UV-lock and hash-free renv restore paths allow independent locked-package work
without changing that convention. The maintainer approved continuing along those
paths on 2026-10-10, with no additional digest-bearing lock files. Keep old
submission and reference assessment paths until their replacements qualify, and
keep acceptance issues open until their own complete criteria are fulfilled.
