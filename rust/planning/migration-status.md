# yamaa Rust engine migration

Assessed 2026-10-09 against main
`3ddbd67342f40ed510bdf2be0b1650dc537ef37f` and the acceptance criteria in
[#1585](https://github.com/elong0527/yamaa/issues/1585).

The goal is to complete one shared Rust implementation behind Python and R,
qualify the supported original-document benchmark and public API matrix, and
retire superseded implementations after their replacements pass. The migration
is active; this assessment does not authorize release cutover or resolve the
explicitly open policy decisions.

## Delivered and qualified

- Shared schema capture, original-YAML preparation, inheritance, compilation,
  source inspection/capture, CSV/Parquet codecs, execution, diagnostics, owned
  results and explicit native publication are implemented. Both hosts install
  one package named `yamaa`.
- The bounded six-document public M1 cohort passes 96 installed tuples across
  direct/source Python forms and the exercised R platforms. Twelve Windows R
  tuples remain explicitly unqualified under #1742. The separate 945-row
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

These PRs passed their complete reviews, every applicable workflow check,
all 16 native CI jobs and all 37 downloaded artifact/source audits. Their
merged source trees equal the qualified heads directly. These results establish
the stated slices; component counts are not a migration percentage.

## Current implementation frontier

The installed Python/R connection is being qualified through private native
project handles with explicit candidate schemas. These call the shared whole
project build, retain typed attempts and prepared documents independently, and
project bounded lossless activation evidence without repeating host effects.
Complete project reports, output admission and public
`domain(..., environment=...)` / `check(..., environment=...)` remain the next
integration gate. Authoritative environment/function formats and the five locked
function benchmarks have not yet been replaced. Installed public-environment
qualification remains pending.

Continue with the following acceptance gates in the
[serial implementation order](migration-order.md):

1. Finish #1757's public environment lifecycle, authoritative formats,
   terminology scope and five installed Python/R function benchmarks. Preserve
   #1753's complete issue and observation contracts and #1755's port order.
2. Complete #1741's producer workflow: compile the graph once, activate before
   data, execute producers once, serialize rounded outputs before consumer
   ingestion, retain failures and publish only through explicit successful save.
3. Complete #1758's shared Define-XML/Dataset-JSON implementation and three
   submission fixtures before retiring the old schema and generator.
4. Close the remaining language/API gaps in #1751/#1752/#1754 and the executable
   inventory, migrate every supported benchmark runner, and qualify complete
   output, issues, logs and exact saved bytes through both installed public APIs.
5. Complete #1740/#1742's numerical, reproducibility, platform and representative
   performance gates. Remove duplicate evaluators and superseded APIs only after
   the approved complete supported matrix passes.

## Decisions still separate from implementation

The pending declaring-file-only path policy has not been adopted: current
approved roots, configuration and fallback behavior remain. The numerical policy,
release representations, Windows R scope, Cargo reproducibility and performance
budgets still require their explicit decisions and qualification.

AGENTS.md still permits tool hashes only in the existing `python/uv.lock`.
Additional digest-bearing lock files are not authorized. The verified existing
UV-lock and hash-free renv restore paths allow independent locked-package work
without changing that convention. Keep old submission and reference assessment
paths until their replacements qualify, and keep acceptance issues open until
their own complete criteria are fulfilled.
