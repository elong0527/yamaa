# Optional native installation probe

This is a source template. Stage the shared Rust workspace before building:

```sh
python rust/tools/stage_r_package.py /tmp/yamaa-stage/yamaanative
```

Then run `R CMD build /tmp/yamaa-stage/yamaanative` from a temporary output
directory. See [`rust/README.md`](../../rust/README.md) for prerequisites,
installation tests, and the deliberately limited capability of this package.

## Scalar transport probe

`scalar_round_trip(request)` accepts and returns one owned UTF-8 JSON string.

Both JSON APIs (`scalar_round_trip` and `evaluate_numeric`) require one nonmissing
character scalar without attributes. Explicit Latin-1 is decoded to UTF-8;
byte-marked text is rejected. Unmarked or UTF-8-marked text is checked without
locale fallback, repair or normalization. The native entrypoints accept raw
bytes and check the byte budget and UTF-8 before constructing a Rust string,
including when called directly. Encoding marks and `enc2utf8` alone do not prove
that R character bytes are valid UTF-8. Malformed bytes raise a stable error only
after the native call returns; subsequent valid requests still work.
It decodes to a real core value before encoding the result. It does not execute
specifications, evaluate expressions, call project functions or expose tables.

```r
scalar_round_trip('{"protocol":"scalar/1","value":{"int":"9007199254740993"}}')
```

The envelope has exactly `protocol` and `value`. Protocol `scalar/1` has these
closed, single-key value variants:

| Variant | Payload |
| --- | --- |
| `missing` | JSON null |
| `int` | Canonical decimal string in the full signed i64 range |
| `float` | Exactly 16 lowercase hexadecimal binary64 digits, most significant first |
| `str` | JSON string, including empty text and escaped NUL |
| `bool` | JSON true or false |
| `date` | Object with canonical `text` (`YYYY-MM-DD`) and `precision` (`year`, `month`, `day`) |
| `datetime` | Object with canonical `text` (`YYYY-MM-DDTHH:MM:SS`) and `precision` (`day`, `second`) |

For example, missing is `{"missing":null}` and negative zero is
`{"float":"8000000000000000"}`. Nonfinite float bit patterns normalize to missing
under REQ-0006. Temporal precision survives this transport; this is not the
language's conversion-to-text boundary. An absent source record is not a scalar
and has no variant here. Integers never pass through an R double or NA sentinel.

Unknown/duplicate fields, numeric JSON integers, malformed payloads, invalid
civil fields and unknown versions are rejected. Requests are limited to
1,048,576 UTF-8 bytes before JSON parsing. This is a prototype transport policy,
not a language limit. Returned data is owned; it does not borrow the request.
Invalid input returns normally from Rust before the R facade raises a condition.
No JSON package is required in R. Callers keep the envelope when exact ordinary
R representation is unavailable; lossy conversion is not performed implicitly.

The same probe is `yamaa_native.scalar_round_trip` in Python. Both installed
packages replay 52 independent positive/negative vectors, reuse results after
input release/garbage collection, and recover after invalid requests. This
qualifies copied scalar text only, not Arrow buffer ownership, callbacks,
diagnostic transport, dataset execution, R Windows installation or CRAN safety.

## Numeric application prototype

`evaluate_numeric(request)` in R and `yamaa_native.evaluate_numeric(request)` in
Python execute the same bounded compiler and numeric lifecycle service. Inputs
are explicit normalized bindings; the call never discovers files, calls host
functions, executes tables, publishes output or falls back to another evaluator.
This is an opt-in prototype API, not the current-schema dataset backend.

```json
{
  "protocol": "numeric/1",
  "expression": "A + 1",
  "column_path": "columns.A",
  "target": "int",
  "bindings": [{"name": "A", "value": {"int": "7"}}],
  "math_policy": "reference_subset",
  "unconvertible": {"value": {"missing": null}}
}
```

The required fields are `protocol`, `expression`, `column_path`, `target` and
`bindings`. Targets are `str`, `int`, `float`, `date` or `datetime`. Each binding
has exactly `name` and a scalar/1 `value`; duplicate/empty names are invalid.
An unbound name is absent, distinct from a binding with a missing value.
All supplied bindings are validated before compilation, including unused ones.
A request has at most 1 MiB of UTF-8 JSON and 4,096 bindings. Core parser/compiler
budgets also apply; these prototype resource policies are not language limits.

`math_policy` defaults to `reference_subset`, which rejects EXP/LN/POWER before
resolution. `portable_libm_v1` explicitly opts into the pinned math policy and
its documented differences from historical Python platform math. It never
silently changes policy. Unknown versions, fields, policy names and duplicate
fields are invalid transport, as are noncanonical or out-of-range runtime scalars.

Omit `unconvertible` for no handler. A present handler must have the shown `value`
wrapper; a bare JSON null is rejected. A wrapped missing value is an explicit
null replacement. Handlers accept primitive literals only: missing, str, int,
float and bool. Use temporal text literals for date/datetime replacements. A valid
but unconvertible literal is only converted when the handler fires; decoding its
wire type is not result conversion. Numeric failures never fire this handler.

Responses have `protocol`, `math_policy`, `outcome`, `handler_counts` and
`resolutions`. Every invocation owns fresh state. `resolutions` lists each reached
name in written order, including repeats. Counts are decimal strings with their
handler name and specification path. A successfully compiled plan registers its
handler at zero before evaluation; compile failures have no registered handlers
or resolutions. A failed replacement remains counted once and preserves the
original result-conversion diagnostic. No result is published by this call.

The closed outcome `status` vocabulary is:

- `value`: `value` uses the scalar/1 codec, after column conversion and handling.
- `failure`: `diagnostic` carries phase, condition, requirement, specification
  paths and complete typed context. A replacement failure also carries `original`.
- `unsupported`: written unsupported functions and name spans, with original
  expression and specification path. No identifier has been resolved.
- `limit`: parser/compiler resource name, limit, optional required count and
  position, with original expression and path. It is not a language failure.

Diagnostic context values use the same scalar codec. The additional
`{"integer":"..."}` form is diagnostic-only canonical decimal integer data;
it is never a valid runtime binding. This preserves out-of-range parsed integer
conversion errors independently of R/JSON number limits. Arithmetic/literal
integer-overflow context retains the reference's decimal string value and
full i64 bounds. Invalid POWER retains the promoted binary64 argument bits.

Grammar diagnostics use the normalized `.derivation.value.compute.expr` path;
evaluation diagnostics use `.derivation.value.compute`; result conversion uses
the column path and replacement conversion uses `.derivation.unconvertible`.
Grammar `position` has separate byte and Unicode-scalar offsets. Evaluation
`source_span` is half-open UTF-8 bytes, and `operand_route` retains structural
`left`, `right`, `unary` or `argument:N` steps. Offsets/counts are decimal strings.
Extra source geometry is outside the reference diagnostic context.

Malformed transport raises a host error; language failures/unsupported/limits
return structured outcomes. Unwind containment covers the shared adapter, not
allocation failures, process aborts or arbitrary host callbacks. This API handles
only normalized scalar requests. Whole-specification preflight, datasets, Arrow,
callbacks, workflow publication, release locking and default cutover remain gates.

## Lossless table interchange

`table_round_trip(request)` accepts a raw Arrow IPC stream and returns sanitized,
owned raw IPC. `table_snapshot(request)` returns exact typed values as table/1 JSON;
i64 never passes through an ordinary R integer/double vector. Neither function
requires the R Arrow package or Python. Inputs use the closed precision-bearing
schema documented in rust/TABLE_TRANSPORT.md in the source repository. Stream
input and output are capped at 8 MiB; snapshot output is capped at 16 MiB, with
additional row/column/batch/cell limits. These APIs do not execute specifications.

## Lossless host scalars

`int64("9007199254740993")` preserves a full-range signed integer, and
`utf8_scalar(as.raw(c(97, 0, 98)))` preserves NUL-containing text. Both are
validated present scalars, distinct from missing. Integer arithmetic and scalar
comparisons use shared Rust semantics; conversions to ordinary R types must be
exact or fail. Use `utf8_bytes` for lossless text bytes. See
[the scalar boundary contract](../../rust/R_SCALARS.md) and `?int64` for supported
operations and explicit encoding rules.

## Explicit function callbacks

`invoke_function(request, callback)` executes one already-bound R function using
the shared engine's function/1 signature and argument protocol. Arguments arrive
in declaration order under their mapped names: `yamaa_int64` for every int,
`yamaa_utf8` for every str, native double/logical for float/bool, logical NA for
missing, and Date/UTC POSIXct for temporal values. Use `as.character` for native
NUL-free text, `utf8_bytes` for all text, and exact scalar operators for integers.

Ordinary R integer and character scalar results are also admitted. Unknown
classes, collections, attributes, invalid encodings, fractional epochs and
non-UTC datetimes are rejected as invalid results; booleans cannot be declared
results. Native NA/nonfinite values normalize to missing before nullable-result
checks. Host temporal arguments lose collected precision at the specified
boundary; returned Date/POSIXct values have day/second precision.

The explicit function is called once on the R thread. Ordinary errors become
fatal function_call_failed outcomes; return-admission errors instead become
invalid_function_result. Interrupt conditions are raised after native return.
Effects are never retried or rolled back, and nested calls are permitted. Input,
result and condition-detail budgets follow
[the shared protocol](../../rust/FUNCTION_TRANSPORT.md); they do not limit the
callback's own allocations or execution. Caller labels do not verify artifact
membership. Environment activation, specification execution and full release
qualification remain open.

## Explicit typed dataset execution

`execute_dataset(request, source)` invokes the shared bounded `dataset/1` service
with JSON plan text and raw canonical IPC. It returns owned IPC only for an accepted
dataset, alongside exact JSON observations; failures never return table bytes.
No R Arrow or JSON package is required. This is a temporary typed-plan bridge,
not specification compilation or automatic backend selection. See
[`rust/DATASET_TRANSPORT.md`](../../rust/DATASET_TRANSPORT.md) for the closed scope,
resource policy, synchronous control behavior, data-bearing diagnostic identities
and remaining integration gates. Python remains the default backend.

`dataset_capabilities()` returns the shared typed protocol and additive feature
names as JSON text before source loading. `row_filter` denotes explicit typed
row-template predicates in [dataset/1](../../rust/DATASET_TRANSPORT.md), including
Boolean logic, comparisons, null tests, IN, BETWEEN and Unicode LIKE. The R package
still has no normalized-specification frontend and does not change backend defaults.
