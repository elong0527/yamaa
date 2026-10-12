# ODM implementation evidence

This slice advances [#1982](https://github.com/elong0527/yamaa/issues/1982).
Public inventory, installed-host qualification, and the complete native matrix
remain acceptance gates before that issue can close.

The authored compiler selects ODM source interpretation from item expressions.
All eleven fields bind with ASCII-only case folding, in the REQ-1266 schema
order. Missing fields retain canonical order; ambiguity and field-type findings
retain stored field order. Declared types/schema and ordinals keep REQ-1275 and
REQ-1295. Required identifiers are checked in fixed field order before binding,
with CSV records counted from two and Parquet records from one. Missing repeat
keys and missing values remain admitted.

CSV parsing keeps the complete syntax/width checks and projects admitted ODM
fields. Parquet admits fixed-field metadata before values, validates every
physical chunk, and retains only the eleven canonical text fields. Nested,
boolean, binary and numeric vendor fields are never exposed or logically typed.
The ordinary closed-profile decoder keeps its own admission behavior.

Item identifiers split at the first dot, retaining the complete remaining OID.
Implicit-key reads select their feeding records and report output-key scope.
Record reads select the original occurrence on the eight hierarchy fields;
group reads select the original relation on the hierarchy fields included in
the enclosing grouping. Grouping on ItemOID or metadata does not further narrow
the item read. Event, form and item-group qualifiers and source-only predicates
apply after scope and item selection. No matching record is missing, one record
returns its Value, and two records fail even when their values agree. Complete
first-appearance identifying differences distinguish repetition from ambiguity.

Column defaults inherit each enclosing row template's scope and identity.
Overrides retain their ordinary authored behavior. Named-intermediate and
aggregate-derive item reads, wrong-driver reads, unknown inputs, invalid filter
names and malformed predicates receive complete static findings before source
capture. Deferred column defaults retain bounded, shared row-template metadata
when their ODM branches depend on completed dataset values, including windows.
The completed-column executor borrows the original admitted driver table,
selects the actual candidate's occurrence or group scope, and keeps the authored
template identity in findings. Nested ODM operands share this metadata rather
than copying it per column; admission rejects a forged driver, template count,
or incomplete group scope before cells. Scope traversal retains the dataset
expression depth and work bounds.

Eight engine controls pin exact item/identifier read order, unchanged opaque
payload pointers, no retry, no Value reads during ambiguity classification,
required-field coordinates, suppression of partial findings on later access
failures, retained snapshots, and whole source-cell admission before cell reads.
Four more engine controls cover distinct occurrences after row numbers,
secondary-driver ownership with an unread poison primary, unchanged opaque
Value failures and leaf paths, and forged deferred-scope admission. Core
controls cover whole metadata/projection admission, fixed field order and
ordinary literal/dictionary spellings that cannot select ODM interpretation.

Fourteen independently authored fixtures compare the complete report, source
table, read ledger and exact CSV bytes, including repeated publication without
another capture. Eight additional fixtures compare complete preflight failures
without resource authority. Six PyArrow containers independently test canonical
projection, ignored nested vendor fields, malformed discarded physical data,
type/ambiguity/missing-field findings and zero-row schema retention. Their data
and expectations were authored independently of the Rust evaluator. Historical
benchmark input and expected files remain unchanged.

Eight additional independently authored reports and CSV artifacts cover the
completed-window defaults, missing items, group selection and ambiguity,
source selectors and predicates, nested string operands and unselected CASE
branches. They are staged alongside the original fourteen for both installed
hosts. The supplemental catalog records each actual contract consumer, including
the six native Parquet controls, without claiming installed execution from
catalog registration alone.

Fresh Python and standalone R execution reuse the same literal fixtures.
The complete public inventory must be reconciled against its immutable plan,
and both hosts must agree on complete observations and remaining unsupported
operations. A passing native unit or focused suite does not retire the public
probes, qualify a different source head, or establish the public ratchet.
