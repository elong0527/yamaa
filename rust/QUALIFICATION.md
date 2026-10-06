# Executable migration coverage

Issue [#1738](https://github.com/elong0527/yamaa/issues/1738) tracks the bridge
from installed component probes to complete benchmark qualification. The
`yamaa.adapters.qualification` reporter consumes the existing conformance report
format and calls its golden and portable observation comparators. It never runs
an engine, rewrites the execution manifest, or generates expected artifacts.

## Scope of the first inventory

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
The first inventory integration supplies reference runs only. Missing native
reports are `not_exercised`, not inferred passes or unsupported language claims.
Connecting the existing native Python path, naming its concrete prerequisites,
registering supplemental probes and executing native coverage in CI remain #1738
follow-ups. R currently lacks the complete original-YAML entry point targeted by
[#1739](https://github.com/elong0527/yamaa/issues/1739).

## Evidence and gates

Each batch identifies its host/backend, qualification level, exact checked-out
source revision, host package and Rust core versions, artifact reference, report
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

Existing executable Python entries must pass. Every mismatch, malformed report,
stale revision and infrastructure failure fails the inventory. Unqualified native
scope stays visible without making migration CI permanently red. A JSON array of
`[fixture, host, backend, level]` entries passed as `--required` additionally gates
the committed qualified set. Missing/retired fixtures, absent reports, unsupported
results and a downgrade from shared to reference-assisted execution fail that gate.
Intentional fixture retirement requires reviewing the manifest and required set.

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
uploads both the input reports and resulting inventory. Native batches must be
produced from real installed executions before their reports can establish
coverage. Unit tests deliberately use synthetic reports to test the reporter's
failure gates; those reports are not native execution evidence.

## Preserved migration history

[Issue #1585's prior detailed plan](planning/archive/issue-1585-through-1737.json)
is preserved as the exact issue body plus title, URL and retrieval metadata before
its next-wave condensation. The JSON uses escaped Unicode for lossless text
preservation. Historical owner-merge/review limitations remain historical; this
archive does not upgrade their qualification claims.
