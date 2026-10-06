# Executable migration coverage

Issue [#1738](https://github.com/elong0527/yamaa/issues/1738) tracks the bridge
from installed component probes to complete benchmark qualification. The
`yamaa.adapters.qualification` reporter consumes the existing conformance report
format and calls its golden and portable observation comparators. It never runs
an engine, rewrites the execution manifest, or generates expected artifacts.

## Scope of the inventory

Every directory with `spec*.yaml` must have exactly one entry in
`benchmarks/execution-manifest.yaml`, and every entry must have a directory.
The inventory records all fixtures for Python/Python, Python/Rust and R/Rust.
It retains the manifest's existing status and runtimes alongside observations.
The denominator is discovered each run; it is not a constant of 315.

Qualification levels describe the path that actually ran:

- `reference_run`: the ordinary Python runner, including its planner.
- `reference_assisted_run`: native execution with temporary Python semantic
  loading/planning. This does not qualify shared compilation.
- `shared_run`: the original YAML ran through shared compilation and execution
  from the installed host. This label requires evidence from that runner.

Typed-plan, scalar and compile-only probes are not accepted as benchmark runs.
Their existing installed tests and capability documents remain separate evidence.
The reference CI matrix reports its ordinary Python runs. The native Python
matrix separately exercises every current fixture from installed host/native
packages, outside the checkout, after both wheel installation and source rebuild.
It compares independent expected artifacts and complete reference observations.
Missing native reports are `not_exercised`, never inferred passes. R currently
lacks the complete original-YAML entry point targeted by
[#1739](https://github.com/elong0527/yamaa/issues/1739); `missing-routes.json`
records that blocker explicitly. The [supplemental contract inventory](SUPPLEMENTAL.md)
reconciles component/compile contracts and installed probes separately from the
benchmark count. The [M2 prerequisite map](planning/m2-prerequisites.md) identifies
the shared compilation work remaining for the fixed six-case cohort.

The current committed candidate set contains 76 Python-assisted native passes:
16 positive and 60 negative fixtures. Some negatives fail in Python frontend
validation before native evaluation, so this is qualification of the complete
assisted route, not evidence that Rust owns those validation semantics. The
remaining 234 fixtures return Unsupported, and five retain explicit timing
mismatches described below. No route is promoted to `shared_run`, no engine
readiness flag changes, and Python remains the default.

## Evidence and gates

Each batch identifies its host/backend, qualification level, exact checked-out
source revision, host/binding package and Rust core versions, artifact reference, report
directory and run evidence. Artifact references are identities; no content digest
is introduced. CI records `git rev-parse HEAD`, including the synthetic merge
revision when that is what the runner tested. All batches in an inventory must
name that same revision. Retain the batch and reports together when archiving a
run; absolute report-directory paths describe the original run location.

The reporter distinguishes five results:

| Result | Meaning |
| --- | --- |
| `pass` | Independent expected truth matches; native reports also match a passing reference report |
| `semantic_mismatch` | Artifacts, diagnostics or portable observations differ |
| `unsupported` | The engine reported unsupported scope; never a negative-fixture pass |
| `not_exercised` | No complete-run report was supplied for this fixture and route |
| `infrastructure_failure` | Execution, report validation or comparison infrastructure failed |

Existing executable Python entries must pass. Malformed reports, stale revisions
and infrastructure failures always fail the inventory. Unlisted semantic
mismatches also fail. A JSON array of `[fixture, host, backend, level]` entries
passed as `--required` gates the committed qualified set. Missing/retired fixtures,
absent reports, unsupported results and a downgrade from shared to assisted
execution fail that gate. Intentional retirement requires reviewing both the
manifest and required set.

`known-gaps.json` records only exact, explicitly unqualified native differences,
with a tracked issue and blocker. These rows remain `semantic_mismatch`, remain
in the summary and can never satisfy a required-pass gate. Every finding must
match, including expected/actual values; an added, changed or disappearing
finding fails CI until its disposition is reviewed. Reference failures and
infrastructure failures cannot be exempted. This keeps unfinished migration
scope visible without turning a known gap into qualified behavior.

For five negative fixtures, the reference route reads one source before reporting
the diagnostic; native validation reports the same diagnostic before source
capture: `negative-formula-flag`, `negative-row-aggregate`,
`negative-row-no-prior`, `negative-source-missing-field` and
`negative-source-trivial-filter`. Their source capture and source-table counts
differ. Those traces are retained unchanged;
#1585 still owns resolving the compatibility difference. Improving one requires
removing its exact gap and adding its passing route to `required.json`.

The summary groups by host, backend, qualification level and benchmark naming
family (`adam`, `sdtm`, `schema`, `negative`). Detailed records retain the actual
findings and stable blocker identifiers. This is an execution inventory, not a
percentage of language semantics implemented.

## Invocation

Run conformance first and supply a batch JSON file with fields matching the
`Batch` model. `reports_dir` must contain only the versioned conformance reports,
named `<fixture>.<host>.<backend>.json`. Package/runtime versions also remain in
each individual report. Use a separate directory for each route.

```sh
python -m yamaa.adapters.qualification \
  --examples-root benchmarks \
  --source-revision "$tested_revision" \
  --batch /tmp/reference-batch.json \
  --output /tmp/qualification/coverage.json
```

The Python CI matrix generates this batch after running all benchmarks and
uploads both reports and inventory. Native CI invokes `rust/tools/qualify_native.py`
with staged fixtures, schema and [committed gates](qualification/README.md).
It verifies that both imported packages come from installed distribution records,
records exact wheel/source filenames under run/attempt artifact identities, and
preserves both package distributions and qualification evidence. The Python host
wheel and both native package forms are retained separately.

For a single opt-in native report:

```sh
python -m yamaa.adapters.conformance schema-functions \
  --backend rust --examples-root benchmarks --run-dir /tmp/native-conformance
```

Conformance report `0.3.0-draft` requires `source_reads`: ordered resource capture
attempts during study ingestion, including cached and failed captures. They record
written path, relative base directory, outcome/condition and snapshots created.
Metadata/specification loading precedes this observation window; these are port
captures, not a trace of every filesystem syscall. The observer delegates to the
approved resource port without rereading bytes. Actual study callbacks are
observed after activation vectors; vectors are excluded. Error reports retain
available callback, source and verification prefixes. Inventory `1.1` retains
known-gap and missing-route declarations. Regenerate older report envelopes
explicitly; missing new fields do not silently become empty observations.

The runner chooses the whole backend before study effects. Native dataset
execution uses the existing temporary Python loader/planner bridge, never the
reference executor as fallback. Producer workflows are explicitly unsupported
before activation and study ingestion. Tests cover independent truth, callback
counts, failure traces, expected-file isolation and absence of fallback. Synthetic
reports used in reporter unit tests do not constitute native execution evidence.

## Preserved migration history

[Issue #1585's prior detailed plan](planning/archive/issue-1585-through-1737.json)
is preserved as the exact issue body plus title, URL and retrieval metadata before
its next-wave condensation. The JSON uses escaped Unicode for lossless text
preservation. Historical owner-merge/review limitations remain historical; this
archive does not upgrade their qualification claims.
