# Shared dependency analysis

The bounded compiler services for issue #1585 move dependency-cycle selection,
stable derivation scheduling and column dependency rules into `yamaa-core`. The optional Python native
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

Python planning uses this graph result for intermediate-read cycle checks and
row-derivation cycle/order analysis. The separate column service below owns
column cycle/order analysis, forward-reference and key-dependency rules. Hosts
attach authored source paths and names to core-selected column diagnostics;
other validation, including phase-boundary restrictions, remains host-owned.
Native analysis failures propagate without running Python graph or column-rule
fallbacks. A topologically sortable graph does not waive language rules.

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

## Column dependency rules

`column_dependencies::analyze` implements REQ-0071, REQ-0072 and REQ-0074 over
already-bound output declarations. Both bindings export
`analyze_column_dependencies` with a separate `column-dependencies/1` envelope:

```json
{"protocol":"column-dependencies/1","dependencies":[[1],[]],"keys":[1],"has_rows":false}
```

This request returns `order: [1,0]` and `diagnostics: []` in a complete outcome:
the later key is exempt from the forward-reference restriction and is scheduled
before its reader. Dependencies include every output declaration: `null` means
no column-phase derivation, while `[]` means a derivation without output-column
references. The binder omits source references, retains dependency first-occurrence
order, and supplies keys in authored key order. `has_rows` records whether the
specification has row templates. No names, schemas, source tables or callbacks
cross this boundary.

Completed row-phase values remain in the declaration catalog for forward-reference
and key checks. Their edges are removed before scheduling, so a completed value
cannot delay a reader behind an unrelated column. Rule failures are returned in
the existing order: first the selected cycle; then forward references in column
and written dependency order, excluding that cycle's members and key targets;
then, without row templates, missing key derivations and non-key dependencies in
authored key order. Other cycles do not suppress their forward-reference failures.
Any diagnostic prevents execution, even when a partial schedule is available.

Each diagnostic carries `condition`, `requirement`, `location` and ordered
`columns` indices. Locations select the authored operation, expression or
column-derivation declaration path. The host attaches paths and names without
re-evaluating column rules. Name resolution, derivation inheritance and phase
selection remain in the Python compiler; this is not full shared compilation.

The column envelope uses the same fixed byte/node/written-edge limits as graph
analysis. All declarations and written edges count, including completed-phase
references and duplicates. Limits precede index admission; invalid/duplicate key
indices are transport errors. Duplicate edges affect written diagnostic order,
while scheduling deduplicates them. The normalized Python binder supplies only
first occurrences. The optional frontend captures both services before activation
or data access. An older wheel without the column service yields explicit
`native_column_dependency_analysis` unsupported status before either effect.

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
and that an absent or noncallable service returns explicit
`native_dependency_analysis` unsupported status before activation or data access,
including when the separately installed wheel predates this service. Additional
installed cases pin row order, completed-phase reads, column/intermediate cycle
provenance, forward-reference rejection and graph-limit outcomes. The existing
installed project tests retain independent activation and callback trace truth.
Twenty additional hand-authored column cases replay through core, transport and
both installed hosts. They pin diagnostic priority, authored key/dependency order,
key exemptions, multiple cycles, completed-phase scheduling and row-template
exceptions. Installed Python planning also checks the actual diagnostic paths,
contexts and order against explicit truth (including the unchanged default planner),
service capture, older-service refusal, failure propagation and explicit retry.
CI repeats installed tests for the wheel and independently rebuilt source archive,
and tests the R source package outside the checkout.

An installed project-function regression injects a graph-analysis failure after
activation and source loading. The original failure propagates once, no dataset
execution or data callbacks occur, and an explicit subsequent attempt reads the
sources again and matches the original CSV and full callback trace. The activation
cache retains successful vector qualification, not dataset success, so that retry
does not repeat the vectors. There is no automatic retry or rollback of activation
or source-provider effects. A caller using a stateful provider must arrange its
own recovery before requesting another attempt; concurrent coordination remains
the cache owner's responsibility.

This is one compiler service, not a complete shared specification compiler.
YAML/schema loading, normalization, inheritance, name binding, most validation,
typed dataset lowering and host capability selection remain in Python. Shared
workflow compilation, R current-spec execution, broader language coverage,
numerical policy, performance and release/default-cutover gates remain open.
Python stays the default and `execution_supported=false`.
