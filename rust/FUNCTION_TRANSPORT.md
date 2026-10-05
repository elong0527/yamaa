# Installed function callback protocol

`yamaa_native.invoke_function(request, callback)` accepts an explicit Python
callable and a function/1 JSON request. It composes the shared no_std invocation
service with a bounded adapter and calls on the current interpreter thread,
without releasing the GIL or starting a worker. Nested calls are permitted;
there is no retry, rollback, caching, timeout or mid-call fallback.

This is a capability-level API for an already-bound callable. Supplying that
callable authorizes its execution in the caller's process. Identity fields are
caller-supplied labels; they do not authenticate artifact membership or sandbox
project code. Production environment discovery, artifact verification, activation,
static call preflight and specification execution remain separate gates. The
Python dataset backend remains the default and `execution_supported` stays false.

A request has exactly these fields (all are required):

```json
{
  "protocol": "function/1",
  "identity": {
    "name": "add_one",
    "contract_version": "1",
    "implementation_version": "1",
    "call": "project.add_one"
  },
  "parameters": [{
    "name": "x",
    "host_name": "value",
    "type": "int",
    "accepts_missing": false,
    "presence": {"required": null}
  }],
  "returns": "int",
  "may_return_missing": false,
  "arguments": [{"name": "x", "value": {"int": "9007199254740993"}}]
}
```

Parameter types are str/int/float/bool/date/datetime; result types exclude bool.
A parameter's `presence` is exactly `{"required":null}` or
`{"optional":SCALAR}`. Defaults and argument values use the existing scalar/1
codec, including decimal-string i64, bit-string binary64, and explicit temporal
precision. Optional missing defaults require `accepts_missing`. Unknown or
duplicate fields, duplicate argument names, malformed scalar representations,
invalid defaults and invalid signatures are rejected before callbacks. The
engine subsequently owns unknown argument names, requiredness, exact runtime
types, missing short-circuit and result checks as described in
[FUNCTION_INVOCATION.md](FUNCTION_INVOCATION.md).

Signature metadata and defaults are admitted in the adapter's shared
`function_signature` module, separately from supplied argument decoding. This
keeps the existing `function/1` request shape and rejection order while preparing
reuse by dataset callback transport. Host-name inspection borrows the admitted
parameter list in declaration order. This refactor adds no dataset callback
transport or activation capability.

Policy is fixed by the adapter: 1 MiB UTF-8 request, at most 256 parameters and
256 supplied arguments, and at most 1,024 bytes per identity/name field. Python
host names must be ASCII identifiers and not hard keywords. Returned text is
limited to 1 MiB UTF-8. Serialized output is capped at 8 MiB during writing,
including JSON escaping. Host error class/message details retain at most 8,192
UTF-8 bytes each, with `host_details_truncated: true` when a prefix is used.
If exception string conversion fails, the original class is retained and the
message is `<exception message unavailable>`.

These budgets bound admission and adapter output, not execution inside arbitrary
host code or allocations it performs, including exception stringification.
Python already owns incoming objects and may allocate UTF-8 caches before Rust
can inspect them. Process aborts and allocator exhaustion are not recoverable
unwinds. The adapter catches Rust unwinds in preparation/invocation/serialization;
callback effects may already exist when an output, resource or internal error is
returned. A subsequent call is a new invocation, never an automatic replay.

The result is `{"protocol":"function/1","outcome":...}`. A successful outcome
is `{"status":"value","value":SCALAR}`. A fatal normative outcome has status
`condition` and a diagnostic with phase, condition, requirement,
`applicable_handler: null`, and context. Context retains function, contract and
implementation identity plus the engine's exact argument/result facts. Host
exceptions additionally retain call, host_error and host_message. Ordinary Python
Exceptions become REQ-0701; primary BaseException control signals such as
KeyboardInterrupt and SystemExit propagate as the original Python exception.
Malformed/oversized inputs or outputs raise ValueError; internal unwind failures
raise RuntimeError. No condition is repaired by lifecycle conversion.

Python scalar encoding uses None, exact bool/int/float built-ins, str, date and
naive whole-second datetime. Full i64, signed zero, Unicode and embedded NUL text
survive. Nonfinite floats normalize to missing before result validation. Numeric
and temporal subclasses and arbitrary returned objects/collections are rejected;
text subclasses follow the reference's str acceptance. Lone surrogates are not
valid UTF-8. Collected temporal precision drops at argument encoding; a returned
built-in date has day precision and datetime has second precision. The raw API rejects arbitrary Python model objects. The optional facade below
bridges the reference's designated temporal model results without losing precision.

Installed wheel and source tests replay all 42 independently authored invocation
cases through real callbacks, and separately exercise retained arguments, owned
results, repeated failures, nested calls, caller-thread identity, cancellation,
exception rendering, limits and temporal boundaries. Rust adapter tests exercise
strict decoding, bounded output and fake-port panic containment. CI runs the same
installed tests outside the checkout on Linux/macOS/Windows and Python 3.12/3.14.

## Python temporal result facade

`yamaa.adapters.native_functions.invoke_function(request, callback)` is an
explicit optional facade over the same native API. It maps only the reference's
known DateValue/DateTimeValue result models, including their subtypes, into a
private immutable native temporal representation. All other returned objects go
through the existing native admission unchanged; there is no duck typing or
general wrapper-to-value conversion. The native extension does not import the
reference Python package.

The bridge retains all civil fields and collected precision. It never converts
these models through builtin dates/datetimes, which would erase metadata. Exact
integer field storage (excluding bool), tuple shape, calendar/time ranges and
closed precision vocabulary are checked again in Rust. Even a forged Pydantic
model constructed without validation cannot introduce an invalid core value.
Malformed known representations become REQ-0702 at result admission, rather than
being misreported as callback exceptions. Exceptions from the actual callback
retain REQ-0701, and BaseException control signals still propagate. The callback
runs once; extraction failure never causes a retry. The facade requires a native
version with this private bridge and checks that capability before callback effects.

The private native carrier exposes no constructor, mutators or subclassing; it
owns either a validated core temporal or a fixed invalid-representation marker.
It is not a generic object container. Argument encoding is unchanged: temporal
arguments become builtins and drop collected precision at that boundary. Builtin
results still acquire full day/second precision. Known temporal model results
retain their supplied precision, matching the Python reference extension.

Installed wheel/source tests compare independent temporal truth and the actual
reference BoundFunction, explicitly checking precision because model equality
ignores it. Cases cover all precision variants, years 1/9999, known subtypes,
forged models, wrong declared result types, unknown objects, failing field access,
primary callback errors, control signals, immutable ownership and post-failure
recovery. The facade is packaged in a non-editable yamaa wheel for these tests.
It does not select a backend, activate environments or authorize artifact code.

## Installed R callbacks

`yamaanative::invoke_function(request, callback)` accepts one explicit R function
and the same function/1 request. It does not resolve callable names or execute
source text. A package-owned dispatcher calls the function once with only the
mapped arguments; extendr invokes that dispatcher through R_tryEval on the R
thread. No R objects cross worker threads. Nested native calls are allowed.
Native input, argument payloads, results and condition details cross as raw bytes;
Rust never reads arbitrary R character pointers. R request encoding follows
[R_SCALARS.md](R_SCALARS.md), including explicit Latin-1 and strict UTF-8 checks.

| Logical value | R argument | Admitted R result |
| --- | --- | --- |
| missing | logical NA | primitive typed NA or nonfinite numeric scalar; nullable check still applies |
| int | exact yamaa_int64 | same exact class or unclassed R integer |
| str | exact yamaa_utf8 | same exact class or unclassed R character |
| float | double | unclassed double |
| bool | logical | recognized, then forbidden by shared result checks |
| date | Date | exact Date class with one numeric/integer whole epoch day |
| datetime | POSIXct/POSIXt, tzone UTC | same exact classes and one whole epoch second, explicit tzone UTC |

Designated classes represent existing logical scalars, as clarified in REQ-0686;
there is no generic wrapper admission. Every int/str argument has the same host
class across the full range, including i64 extrema, NUL and empty text. Useful
integer arithmetic and comparisons use the shared primitives. Extra attributes,
subclasses, vectors, NULL and other classes are rejected. Date has only its class
attribute; POSIXct has only its exact class vector and single UTC tzone attribute,
in either order. Whole civil epochs within years 1..9999 fit binary64 exactly.
No host timezone or decimal parser converts epochs. Collected temporal precision
drops only during host argument encoding; returns have day/second precision.
Temporal NA/nonfinite storage normalizes to missing after validating its class
and scalar storage, before the logical nullable-result check.

The shared request/result/output budgets also apply here. R host names follow
the environment's ASCII identifier policy, excluding reserved words, `...` and
`..` followed by digits. No callback effects occur before request admission.
The native byte scalar codec caps returned text at 1 MiB; an oversized return
reports an output resource failure after that single callback effect.

A dispatcher catches ordinary R errors around the actual call, separately from
result admission. Primary conditions become REQ-0701 with class/message UTF-8
prefixes capped at 8,192 bytes each; truncated prefixes are marked. Secondary
condition rendering/encoding failures use stable unavailable-detail fallbacks;
invalid bytes are never repaired. A conditionMessage failure raised by base R
before it signals the intended condition is itself the callback's primary error.
Interrupt conditions are retained by the facade and re-signaled after Rust
returns. Invalid result representations become REQ-0702, not call failures.
Unexpected dispatcher structure errors and Rust unwinds become transport errors.
All R errors are raised after the normal native return. No process-abort,
allocator-exhaustion, callback timeout or host-allocation bound is promised.

Source-installed tests outside the checkout replay all 42 shared invocation
cases with actual R callbacks and exact traces. R's out-of-range forged integer
carrier is a representation failure rather than Python's built-in bigint result;
both produce REQ-0702. Additional tests exercise primitive return admission,
NUL/Unicode/Latin-1, malformed encodings/classes, temporal extrema and precision,
missing/nonfinite normalization, argument/result ownership after GC and mutation,
caller process, nested calls, primary interrupts, secondary condition rendering,
UTF-8 detail truncation, zero-effect request rejection, post-effect failures and
subsequent-call recovery. Direct internal dispatcher tests also exercise malformed
raw return admission. CI runs the source-installed tests on Linux and macOS.

Full environment, workflow, benchmark and release gates in #1585 remain open.

The trusted Rust dataset API now composes this shared invocation lifecycle with
explicit callback ports; see [dataset function composition](DATASET_FUNCTIONS.md).
This does not add dataset callbacks to either installed host transport.
