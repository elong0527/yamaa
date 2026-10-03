# Execution conformance protocol

`python -m yamaa.adapters.conformance` executes fixtures without reading
`expected/`. `compare_example` subsequently compares their committed goldens.
`compare_reports` compares two completed executions without running callbacks
again or changing expected artifacts.

## Available execution coverage

| Host | Backend | Coverage |
| --- | --- | --- |
| Python | Python | Cases declaring `python` in `benchmarks/execution-manifest.yaml`. |
| Python | Rust | Not implemented; blocked by #1585. |
| R | Rust | Not implemented; blocked by #1585. |
| R | Legacy R | Package and grammar tests only; no claim of current-schema benchmark execution. |

Accepting a backend label in a report is protocol support, not an implementation
of that backend. The current CLI always runs the Python backend. Unsupported or
infrastructure-error reports never establish parity, even against identical
reports. Add manifest coverage only after an actual runtime executes the fixture.

Rust core primitives and their remaining requirement-level gates are tracked in
[`rust/CAPABILITIES.md`](../../../../rust/CAPABILITIES.md). Those unit tests and
native installation probes do not change this execution coverage table.

## Version 0.2.0-draft

The envelope carries `runtime` (`python` or `r`), `runtime_version` (host language
version), `backend` (`python` or `rust`), and `engine_version` (engine release).
Report filenames are `<example>.<runtime>.<backend>.json`. Version 0.1 reports
must be regenerated: the reader rejects old versions and missing observation
channels instead of assuming that missing information means no activity.

In addition to artifacts, diagnostics, and entry-result handler counts, reports
contain these required channels (empty lists are meaningful):

- `nodes`: executed workflow prefix, per-node outcome, diagnostics, unsupported
  features and handler counts, including producer failures.
- `tables`: ordered ingested sources and successful prepublication derived
  tables for every executed node. Producer tables and columns omitted from output
  are retained. These are table boundaries, not a trace of every transient
  expression or donor selection; failed runs have no invented derived table.
- `verifications`: evaluated declared column/dataset checks in execution order,
  including held checks, warnings, failures and counts even when the specification
  requests no verification sidecar. Key/output failures remain diagnostics.
- `callbacks`: actual study function invocations in order, with logical function
  name/version and declaration-ordered arguments after defaults. Activation
  vectors and missing-argument short circuits are excluded. The callback list's
  length is the invocation count. Host binding names are not logical identities.

Specification identities are paths relative to the entry specification's parent.
Scalars carry explicit types; missing is distinct from empty text, integers use
decimal strings, and floats use 16 lowercase hexadecimal digits encoding their
big-endian IEEE-754 binary64 bits. This is the value representation, not a hash.
Date/datetime strings describe civil values at the observed table/host boundary.

Comparison preserves all ordering, values, types, callback arguments, verification
records and handler counts. CSV artifacts compare their full text/byte length;
Parquet artifacts compare logical content and omit physical byte length. Host
versions, engine versions and host exception class/message/binding spelling are
not portable equality fields. Conditions, specification paths and the remaining
diagnostic context still compare exactly. No numerical tolerance is applied.

## Comparing a candidate run

Keep exactly one reference report per example in a separate directory:

```sh
python -m yamaa.adapters.conformance schema-lookup adam-adsl-bmi \
  --examples-root benchmarks --run-dir /tmp/yamaa-candidate \
  --reference-reports /tmp/yamaa-reference/reports
```

The command checks both committed goldens and reference observations. A future
adapter can call `read_report` and `compare_reports` directly. A reference report
is differential evidence, not normative expected truth; disagreement must be
resolved against rules and independent edge cases.
