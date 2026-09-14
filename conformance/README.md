# Conformance runner protocol

Protocol 1.0 is the language-neutral boundary between the shared #101 runner
and an R or Python engine adapter. The committed JSON Schema is
[`protocol.schema.json`](protocol.schema.json); the Pydantic models and the
reference comparator implement the same boundary.

The protocol deliberately keeps execution and comparison separate. An adapter
receives no expected-artifact path and must not read any `expected/` directory.
It executes the specification and writes only beneath its isolated runtime
directory. The shared comparator receives `expected/` after both adapters have
finished.

## Runtime directory

For a run root such as `/tmp/yamaa-run`, the shared runner gives each adapter a
separate directory:

```text
/tmp/yamaa-run/
  r/
    invocation.json
    report.json
    resolved-specification.json
    artifacts/primary.csv
  python/
    invocation.json
    report.json
    resolved-specification.json
    artifacts/primary.csv
  summary.json
```

The adapter command takes exactly one positional argument, the path to
`invocation.json`. It writes `report.json` at the fixed location inside the
declared `output_directory`. Artifact paths recorded in the report are relative
to that directory, normalized with `/`, and cannot escape it.

## Invocation

One invocation executes one entry specification in one runtime:

```json
{
  "protocol_version": "1.0",
  "run_id": "sdtm-dm-basic-001",
  "example": "sdtm-dm-basic",
  "runtime": "python",
  "project_root": "/checkout/yamaa",
  "schema_root": "yaml",
  "entrypoint": "yaml/examples/sdtm-dm-basic/spec.yaml",
  "data_roots": [],
  "output_directory": "/tmp/yamaa-run/python"
}
```

`project_root`, the optional `data_roots`, and `output_directory` are absolute
host paths. `schema_root` and `entrypoint` are normalized paths relative to the
project root. The R and Python invocations have identical case fields and
differ only in `runtime` and `output_directory`.

Expected paths, expected values, and expected error fields are intentionally
absent. An adapter that cannot execute without them does not implement this
contract.

## Report

Every report identifies the exact invocation bytes by SHA-256 and records:

- runtime language and implementation versions;
- every authored specification document and its SHA-256;
- the resolved authored data tree, when resolution succeeded, as a JSON file;
- every consumed source under its dataset name, written path, and SHA-256;
- every declared handler path and count, including zero counts; and
- one explicit outcome: `success`, `failure`, `unsupported`, `blocked`, or
  `infrastructure_failure`.

The resolved JSON represents the YAML 1.2 data tree after R017 composition and
before R006 shorthand expansion or defaults. JSON object order is immaterial;
array order is preserved. The comparator verifies each file digest and compares
the decoded trees, so R and Python serialization whitespace cannot create a
false difference.

A successful outcome contains one primary artifact. Its table observation
records columns and rows in artifact order. Each cell separates missingness
from a non-missing value, so missing and the empty string cannot collapse:

```json
{"missing": true}
{"missing": false, "value": ""}
```

Values use these portable representations:

| Column type | `value` representation |
|---|---|
| `str` | exact string |
| `int` | canonical decimal text |
| `float` | 16 lowercase hex digits containing the IEEE-754 binary64 bits |
| `date` | R016 canonical date text plus `precision`: `year`, `month`, or `day` |
| `datetime` | R016 canonical datetime text plus `precision`: `second` |

The out-of-band temporal precision is required because R016 deliberately does
not render collected precision into an artifact. Table equality therefore
checks runtime types, values, missingness, row and column order, binary64 bits,
and collected precision independently of container serialization.

A failed outcome contains structured diagnostics only. `phase`, `condition`,
`spec_paths`, `requirement`, and declared context are portable. Exception class,
stack trace, and message text never enter the report. Unsupported execution and
missing prerequisites use their own outcomes; neither can satisfy a negative
semantic fixture.

## Handler-count golden

An executable specification that declares handlers has
`expected/handler-counts.yaml`. Keys are entry paths relative to the example:

```yaml
version: "1.0"
specifications:
  spec.yaml:
    - spec_path: columns.VALUE.derivation.mapping.missing
      handler: missing
      count: 0
```

The comparator treats `(spec_path, handler)` as the identity and requires the
exact count. A report containing handler counts without this committed contract
fails rather than silently accepting an unpinned observation.

## Adapter exit behavior

An adapter returns:

| Code | Meaning |
|---|---|
| `0` | `report.json` was written and validates, for any report outcome |
| `2` | the invocation was invalid; no engine execution occurred |
| `3` | infrastructure failed; write an `infrastructure_failure` report when possible |
| `4` | the report could not be published |

`failure` is a semantic engine result, not a process failure, and therefore
returns zero. The comparator decides whether that result matches a negative
fixture. `unsupported`, `blocked`, and `infrastructure_failure` are visible
non-passing results for an executable manifest entry.

## Comparison

After both adapters exit successfully, compare the reports and committed
expectations:

```bash
uv run --project python --no-sync python -m conformance compare \
  --run-root /tmp/yamaa-run \
  --expected-root yaml/examples/sdtm-dm-basic/expected \
  --runtime r --runtime python \
  --summary /tmp/yamaa-run/summary.json
```

Protocol 1.0 compares committed CSV artifacts byte for byte, as R020 requires.
It also validates observed column types against the resolved specification,
compares the typed tables and provenance between runtimes, matches a negative
fixture's required diagnostic fields and context, and checks handler counts.
Parquet observations are representable for parity, but promotion under this
version still requires the repository's committed CSV golden.

The command returns zero only for a passing comparison, one for a comparison
failure, and two for invalid command-line use. The summary is written even on a
comparison failure and contains no host exception or stack trace.

Check the models, schema, and comparator from the repository root:

```bash
uv run --project python --no-sync python -m conformance schema check
uv run --project python --no-sync python -m pytest conformance/tests
```
