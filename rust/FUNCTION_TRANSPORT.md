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
built-in date has day precision and datetime has second precision. The reference's
DateValue/DateTimeValue wrapper-result extension is not yet exposed by this native
API; facade compatibility remains a tracked integration gate.

Installed wheel and source tests replay all 42 independently authored invocation
cases through real callbacks, and separately exercise retained arguments, owned
results, repeated failures, nested calls, caller-thread identity, cancellation,
exception rendering, limits and temporal boundaries. Rust adapter tests exercise
strict decoding, bounded output and fake-port panic containment. CI runs the same
installed tests outside the checkout on Linux/macOS/Windows and Python 3.12/3.14.

R callback integration remains next. R raw IPC already preserves all scalar
values, but ordinary R integers/doubles cannot carry every i64 and R character
strings cannot carry embedded NUL. Callback scalar admission needs an explicit
lossless representation or explicit unsupported outcome; neither silent narrowing
nor a false claim of full R callback parity is acceptable. The full environment,
workflow, benchmark and release gates in #1585 remain open.
