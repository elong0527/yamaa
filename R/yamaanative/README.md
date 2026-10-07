# Optional native installation probe

`decode_yaml(raw_source)` exposes the experimental shared
[YAML byte decoder](../../rust/YAML_DECODING.md). It returns an ordered
schema-compatible document arena with exact integer strings and float bits,
or source diagnostics and explicit resource refusals. It does not use R's
YAML/numeric conversion or provide a complete current-schema workflow frontend.

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

The additive `grouped_count` capability accepts a grouped assignment
`{count: {column: index_or_null, text: original_expression}}` through the typed
dataset entry point. A null column counts records without reading a stored field;
an index counts present values of any column type. No selected records yields
missing, whereas an existing all-missing field group counts zero. Empty text is
present. This is bound-plan execution; the R package does not yet compile
current-schema specifications. See [the transport contract](../../rust/DATASET_TRANSPORT.md).

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
also advertises `predicate_checks` for typed assert checks, with an optional `when`,
over completed output columns. These validate declaration types even for empty output,
evaluate `when` and `require` eagerly and retain earlier check records on a predicate condition.
The `key_grain` feature additionally supports standalone key combinations with
complete source memberships, first-occurrence order and exact raw-value conflict
diagnostics. Missing identities remain separate until output validation. The package
still has no normalized-specification frontend and does not change backend defaults.

The additive `window_numbering` dataset feature accepts unfiltered row numbering
and competition/dense rank in the key-grain column phase. Order terms bind completed
output columns with explicit direction and null placement; no R callback or source
read occurs per comparison. Shared fixtures cover wide integers, ties, missing
partitions, stable output order and typed empty output. This remains a typed-plan
bridge; it does not provide an R specification compiler or full window execution.

`window_filter` adds optional scalar eligibility predicates to these numbering
windows. Filtering preserves output rows and gives excluded rows missing values;
only true rows contribute positions or ties. Rust evaluates each reached partition
before converting its first result. Shared tests cover true/false/unknown eligibility,
eager predicate conditions with operation provenance, and empty-output behavior.

`window_values` adds completed-output donor reads through the `window` expression:
`row_value` counts eligible offsets, `previous_non_missing` reads the nearest earlier
present donor, and `locf` retains the current present value or carries the prior one.
Donor indices are cached per partition, with text copied only for each requested
result under the existing budgets. Shared raw fixtures exercise gaps, filtered
donors, full-range signed offsets, exact large integers and typed empty outputs.

`window_baseline` adds `baseline_flag` for completed date/date or datetime/datetime
columns with no ordering. Rust compares each eligible row's date with its own
reference, skips missing operands and marks the unique latest candidate with `Y`.
Ties return REQ-0322 with exact count, canonical date text and a separate `partition`
list of named typed grouping values. Shared fixtures cover per-row references,
missing dates, a three-way tie, filtering and empty input. This remains the R
bound-plan bridge; a normalized-specification frontend is still outstanding.

`root_filter` admits a source-only predicate on the sole key-grain template.
It evaluates all source rows before any key conversion, retains only true rows
and keeps original row coordinates for subsequent collected reads. False/unknown
rows cannot contribute conflicting values. Shared raw fixtures cover this scope,
a predicate failure before a bad key, complete exclusion and empty-input behavior.

`source_filter` adds a source-only predicate to a collected non-key reading.
It narrows that output row's feeding records after key construction and root
filtering. Every eligibility predicate finishes before donor values are read;
only true records contribute distinct present values. No qualifying value returns
missing without removing the output row. Shared fixtures retain exact conflicts,
primitive condition provenance and empty-input behavior. Keys, record/group
assignments remain outside this filtered-read slice.

`source_selection` permits optional `collect.selection` with source-indexed
`order_by` terms and `keep: "first" | "last"`. Only multiple distinct present
values invoke the ordered choice, after source filtering. Equal values retain
their first representation and emit no count. The raw envelope includes optional
`handler_counts` after `outcome`, with exact decimal-text counts, source handler
paths and `multiple_matches` names. Completed choices remain visible if later
conversion, checks or resource limits fail; repeated execution starts fresh.
Shared fixtures replay first/last choices, filtered uniqueness, empty inputs and
a later conversion failure with its retained count. Other handlers are unsupported.

`execute_dataset_sources(request, source, secondary)` additionally accepts a
list of raw secondary IPC snapshots in the plan's `secondary` schema order.
The `multi_source` capability permits bound key-grain non-key `lookup` reads.
Missing keys never match; absence yields missing, while duplicate matching
records fail with exact match count and named match-key evidence even when values
agree. Combined input is bounded to eight sources, 8 MiB, 262,144 cells and
65,536 rows. This raw API does not introduce an R specification frontend.

`named_intermediate` adds bounded named secondary selectors to the same request.
Source-only filters run once when first needed; same-type keys bind completed
outputs, and optional order/keep selects records with stable ties. Explicit
tagged `no_match` handles absence; omission preserves REQ-0124. Cached record
choices still record inherited `multiple_matches`/`no_match` at every reading's
source path before conversion. Six independent shared cases replay cached reads,
absence, duplicate identical records, empty output, false filters and retained
counts on conversion failure. Broader intermediate policies remain unqualified.

`numeric_compute` connects the same bounded numeric compiler to dataset source
and completed-output bindings. Arithmetic and supported numeric functions retain
exact association, eager operand reads, deferred overflow, source spans and
operand routes. Result conversion follows evaluation; validation conditions have
no output identity, while derivation conditions retain available complete keys.
Six shared cases cover values, arithmetic/type/overflow/conversion failures and
empty input. EXP/LN/POWER, local handlers and broader bindings remain outside this
dataset slice; the separate numeric/1 candidate policy does not enable them here.


`host_functions` adds `execute_dataset_functions(request, source, secondary,
callbacks)` for explicitly supplied functions in declaration order. The adapter
captures stable callables, validates metadata before IPC decoding, preserves
callback order and original interruptions, and withholds output after failure.
The shared scalar port preserves full-range integers and bounds diagnostics.
See [the dataset protocol](../../rust/DATASET_TRANSPORT.md#explicit-host-functions)
for limits and independent installed-host fixtures. This raw bridge does not
provide normalized specification lowering or production artifact activation;
Python remains the default and `execution_supported` remains false.

`function_source_collection` permits `{collect: {column, identifier}}` callback
argument inputs for key-grain non-key calls. All feeding records participate;
missing values are ignored and conflicting present values stop the call with
exact ambiguity evidence. The six additional shared cases replay this behavior
through the installed R bridge without changing existing expected values.

`analyze_aggregate()` exposes the shared `aggregate-syntax/1` compiler service.
It returns syntax, ordered references or portable grammar/resource diagnostics
without requiring Python. See [the transport contract](../../rust/AGGREGATE_SYNTAX.md).
Parsing the closed vocabulary does not enable new dataset execution forms or
provide a current-schema specification compiler.

`analyze_numeric()` exposes `numeric-syntax/1` through the same shared Rust
parser used by optional Python admission and planning. It retains exact literal
text, ordered identifiers and portable diagnostics without evaluating anything.
See [the numeric syntax contract](../../rust/NUMERIC_SYNTAX.md). Parsing
POWER/EXP/LN does not enable their dataset execution or change numerical policy.
