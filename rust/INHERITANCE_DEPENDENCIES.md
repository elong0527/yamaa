# Shared inheritance dependencies

The experimental `schema/1` operation `resolve_inheritance_dependencies` consumes
an admitted, normalized, composed document after non-strict window expansion.
Rust discovers references, retains reachable declarations and stably orders the
surviving columns under REQ-0638-0644. Python's opt-in loader delegates to this
operation before strict window expansion and complete specification validation.
Both installed native hosts expose the same query. Python remains the default;
`execution_supported` remains false.

## Semantic boundary

References follow the first matching schema type. Literal strings, mapping keys
and identifiers outside their declared dataset/column roles do not create edges.
Class fields and registry payloads retain their scope. Numeric, aggregate,
predicate and string-template expressions use closed syntax, without evaluation,
source access, callbacks or a host parser. `COUNT(D.*)` creates a relation read
although it names no field. This also corrects the reference inheritance loader's
previous removal of a dataset used only by a live record-count expression.

Reachability starts with output columns/order, keys, dataset assertions, columns
carrying assertions, base and every row declaration. It follows live column and
row derivations, row drivers/grouping/filters, and live intermediate matching
inputs. Dead columns, inputs, intermediates and dead row assignment targets are
removed. All rows retain their original order; empty intermediates are omitted.
Unused declarations' semantic reference defects remain prunable. Structural
layer admission still occurs before this pass.

Column dependencies include row assignments and bare matching inputs of referenced
intermediates. Unknown dependencies are reported by initial column order and
lexical dependency order before cycle reporting. Stable topological scheduling
chooses the earliest original column position at each step. A cycle reports all
unscheduled columns in original order, including downstream dependents. Artifact
`output.columns` order remains unchanged.

Successful results use the existing `normalized` envelope: every output occurrence
is owned, and its origin names a node in the retained input document. Invalid
results publish no partial document. Cycle diagnostic context uses `text_list`
for its ordered names. Host provenance remains attached to logical paths.

## Resource and error policy

Schema traversal, retained references, graph storage, comparisons and output copies
share the batch's normalization budgets. Syntax scratch is reserved before parsing;
failed attempts keep their charges. Numeric/aggregate/predicate parsers retain
their existing bounded parsing policies. Predicate regex compilation additionally
retains its per-parse finite policy. These are logical resource limits, not a
process-memory, allocator-failure or asynchronous cancellation guarantee.

Malformed closed expressions contribute no dependency edges at this stage, as in
the reference resolver; surviving declarations remain subject to subsequent full
semantic validation. Parser resource refusals and unsupported features instead
stop the operation explicitly. They cannot produce a successful empty graph or
trigger a fallback to host pruning.

## Qualification and remaining work

Six independently authored complete wire cases cover record-count reachability,
stable ordering, row roots and lookup matching, template literal boundaries,
unknown-before-cycle diagnostics, cycle residual ordering and assertion roots.
They are shared by core transport and both installed host test suites. Focused
core tests also exercise first-match unions, namespace roles, malformed syntax,
origin retention and exhausted-budget recovery. No baseline golden is rewritten.

A separate reference regression specifies the exact `COUNT(EX.*)` results of two
and one records and verifies that the unused source is not read. The installed
native loader retains the right declarations with reference dependency helpers
disabled. Qualified key-matched aggregate execution remains unsupported by the
current native executor and is refused before the provider runs. This compiler
change does not advertise that executor capability.

[Shared traversal](INHERITANCE_TRAVERSAL.md) owns parent ordering, deduplication,
cycles and role-specific version checks; filesystem authority stays with explicit
host ports. Full current-schema R workflow loading/execution, separate
POWER/EXP/LN policy, remaining execution scopes, deployment containment and issue
#1585's complete release/default-cutover gates remain open.
