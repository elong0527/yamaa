# Shared dependency analysis

This bounded compiler slice for issue #1585 moves dependency-cycle selection and
stable derivation scheduling into `yamaa-core`. The optional Python native
frontend selects this service before project activation or source-provider
effects and uses it during actual planning. The default Python planner remains
unchanged. Both installed Python and R bindings expose the same service; this
does not provide R current-schema compilation or enable `execution_supported`.

## Contract and ownership

`dependency_analysis::analyze` accepts already-bound nodes indexed in declaration
order. Each node contains the indices it depends on. Binding, normalization,
inheritance, phase membership and reference legality remain the compiler's
responsibility. The host removes dependencies outside the current graph, such
as source fields and values completed in an earlier phase.

The shared service sorts and deduplicates edges, visits roots and dependencies
in declaration order, and returns the first cycle reached by depth-first search.
It rotates that cycle to its earliest declared member without reversing edges,
then repeats the endpoint. This is the first *reachable* cycle, which need not
contain the earliest declared cyclic node in the whole graph. Iterative traversal
avoids host recursion limits.

Scheduling repeatedly selects one earliest declared ready node. A newly ready
earlier node therefore precedes an already waiting later node. Cycles and their
blocked readers are omitted from the returned maximal schedulable order. The
cycle result remains available alongside that order; it is not a transport error.

Python planning uses the result for intermediate-read cycle checks, row-derivation
cycle/order analysis and column-derivation cycle/order analysis. Existing host
diagnostics still own requirement IDs, source paths, contexts and diagnostic
ordering. Forward references, key dependencies and phase-boundary restrictions
are still checked; a topologically sortable graph does not waive those rules.
Native analysis failures propagate without running the Python graph algorithms.

## Installed transport

Both hosts export `analyze_dependencies` with the same strict JSON envelope:

```json
{"protocol":"dependency-analysis/1","dependencies":[[1],[]]}
```

The successful result for that request is:

```json
{"protocol":"dependency-analysis/1","outcome":{"status":"complete","cycle":null,"order":[1,0]}}
```

Admission caps UTF-8 request bytes at 1,048,576 before JSON decoding. Graph policy
allows 4,096 nodes and 65,536 written edges, counting duplicates before removal.
Node and edge limits precede dependency-range validation after a valid envelope.
Unknown/duplicate fields, non-integer indices, invalid indices and unsupported
protocols fail explicitly. Callers cannot raise limits through JSON. The typed
core API accepts trusted explicit limits.

Graph resource limits are separate outcomes with `status: "limit"`, `resource`,
`limit` and `required`; counts are canonical decimal strings. Python's planning
adapter raises `NativeDependencyLimitError` with those counts rather than a
language diagnostic. Malformed transport raises a host error. R validates scalar
text and converts native errors to R errors only after the native call returns.
No request content or panic payload is included in stable native error text.

## Qualification and remaining work

Thirteen hand-authored shared cases pin empty/isolated graphs, chains, diamonds,
duplicates, ready-node tie breaks, cycle direction/rotation/selection and partial
orders. The unchanged Python reference, Rust core, strict transport and installed
Python/R bindings replay that truth; tests do not generate expected graphs from
an implementation. A 4,096-node chain and cycle exercise iterative traversal and
reuse. Admission tests cover shape limits, invalid indices and repeated calls.

Installed planning tests disable the reference graph functions and check actual
native execution against original ADLB, window, lookup and project-function CSVs.
They verify that source-provider mutation cannot replace the captured service,
and that an absent service fails before activation or data access. Additional
installed cases pin row order, completed-phase reads, column/intermediate cycle
provenance, forward-reference rejection and graph-limit outcomes. The existing
installed project tests retain independent activation and callback trace truth.
CI repeats installed tests for the wheel and independently rebuilt source archive,
and tests the R source package outside the checkout.

This is one compiler service, not a complete shared specification compiler.
YAML/schema loading, normalization, inheritance, name binding, most validation,
typed dataset lowering and host capability selection remain in Python. Shared
workflow compilation, R current-spec execution, broader language coverage,
numerical policy, performance and release/default-cutover gates remain open.
Python stays the default and `execution_supported=false`.
