# Typed dataset function composition

The trusted Rust `DatasetPlan` API can compose scalar host-function calls with
row and column assignments using `Expression::Function(BoundFunction)`. This is
an engine port, not a new `dataset/1` wire feature. Python/R dataset callbacks,
normalized-specification admission, environment discovery, artifact verification,
activation vectors and installed cross-host callback qualification remain open
in issue #1585. Python stays the default and `execution_supported` remains false.

## Admission and ownership

A `BoundFunction` owns an immutable `InvocationPlan`, a caller-selected callback
slot, and supplied arguments in authored order. Argument leaves are exact typed
literals, primary-source fields or completed output fields. Construction rejects
unknown/duplicate names and omitted required arguments. Assignment admission
checks field bounds and phase availability. Grouped calls may read grouping
fields only. Key-grain key assignments do not admit calls; non-key key-grain calls
may read completed output columns or literals, never one chosen feeding record.
There are no secondary/named/correlated argument reads or expression arguments.

The caller passes `FunctionBindings` to `execute_observed_functions`. Before any
table method is called, every referenced slot must report exactly the declared
invocation identity and signature, including defaults, host names and result
policy. This check also applies to empty input and filtered-out candidates.
The existing execution entrypoints provide no functions and explicitly fail
with `FunctionBinding` when the plan requires one; they never discover code.

The registry is a trusted adapter port: metadata inspection must not run project
code, mappings remain stable during execution, and the adapter must bind the
correct already-activated callable. Identity equality is not authentication of
project artifacts or a sandbox. Source and callback ports share an opaque error
type; an adapter may distinguish their payloads with an enum. Callbacks execute
synchronously and require neither `Send` nor `Sync`. The engine cannot undo host
side effects or contain host panics, interrupts or process termination; adapters
retain their existing exception/control-flow and allocation responsibilities.

## Observable order

For each reached assignment, supplied arguments are resolved in authored order.
Every argument is resolved before the invocation lifecycle tests exact argument
types or applies non-accepting-missing short circuit. Thus a missing first argument
cannot hide a later source error. The shared `InvocationPlan` then processes
parameters in declaration order, fills defaults, maps host names, and calls once
unless a missing value suppresses the call. There is no retry or result cache.

Template calls execute before that template's filter. Later whole-column calls
execute only for retained candidates, completing one whole column before the
next. Grouped calls run once per group; non-key key-grain calls once per output
key combination. A successful result is checked against the exact function
return type, then converted to the output column type before a dependent call
can read it. Ordinary completed-value conversion handlers can recover that last
conversion only. Callback failures, invalid arguments and invalid results are
fatal and bypass handlers; they retain invocation identity, assignment path and
output identity when available. A failed run exposes no accepted dataset, while
its declared handler counts remain observable. Reusing a plan starts fresh run
accounting and repeats callbacks.

## Limits and qualification

Argument visits and potential calls consume the shared work budget before reads
or callback dispatch. Supplied string copies and returned string processing
consume the shared scalar-text budget; converted output values and diagnostic
identities use existing budgets. Trusted plan metadata, names, literal/default
storage and host-side allocations must be bounded by the compiler/adapter before
this API is used. These logical budgets do not cap callback wall time or memory
allocated inside arbitrary project code.

Independent core tests cover authored-versus-parameter order, defaults, missing
with a later read failure, repeat attempts, empty/filtered rows, grouped and
key-grain candidates, signature refusal before table access, fatal call/result
errors, conversion recovery, converted dependent values and resource exhaustion.
Separate Python reference observations pin argument ordering without generating
expected Rust results. Existing scalar invocation fixtures remain the authority
for temporal encoding and host representation details; this slice does not
claim installed Python/R dataset callback parity or ADSL BMI completion.
