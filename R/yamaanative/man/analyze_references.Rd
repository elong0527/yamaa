\name{analyze_references}
\alias{analyze_references}
\title{Bind and validate references through shared Rust compiler metadata}
\usage{analyze_references(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON with ordered query results, or a resource limit.
Invalid transport or catalog/query metadata raises an error after Rust returns.}
\description{Accepts reference-analysis/1 JSON containing catalog and queries.
The catalog lists ordered outputs (name and type) and datasets (name and ordered
fields, each with name and type). Types are str, int, float, date and datetime.
Binding queries name one reference. Bare references reach only output columns;
qualified references split at the first dot and reach an exact stored field.
ODM items are not inferred from suffixes. No records or callbacks are accessed,
and no Python installation is required.}
\details{A bind query has kind bind and name. A validate_output query has kind
validate_output, name, expected (a type or null), available (output indices or
null for column derivation) and candidates (dataset indices for suggestions).
Validation returns the first applicable condition in unknown-name, phase, then
expected-type order. Suggestions choose the lexically earliest matching candidate
dataset name. Results use zero-based declaration indices; hosts attach names,
row identity and authored paths. Intermediate expression traversal, join inference
and the rest of the language remain outside these metadata services; this is not
a current-schema compiler.

A validate_qualified query has name, optional expected, and scope. Scope contains
drivers (names), current_driver (boolean), reach (scalar, record, relation or
declared), joined (boolean) and phase. A row phase has kind row and optional/null
group_by or a list of complete qualified names. A column phase has kind column
and groups, the group-name lists of that dataset's grouped templates. Results
have kind qualified_validation and ordered diagnostics: driver_mismatch,
unknown_field, row_phase, row_group, column_group or incompatible_input_type.
Driver/existence failures stop; phase/grouping failures retain later type errors.
The host selects authoritative scope metadata and attaches authored paths;
these queries do not grant data access or execution authority.

Scope limits are 256 written driver names, 4,096 groups, 65,536 group-name entries
and 1,048,576 combined UTF-8 context bytes. Duplicate entries count. Query support
is advertised by reference_capabilities(); existing query forms remain unchanged.

The key_relations feature provides comparable_types (left and right declared
types, returning comparable) and infer_keys (ordered keys and typed fields).
Key inference returns key_inference with an inference kind: keys (original key
indices), no_applicable_keys, undeclared_output (key index), or incompatible
(first key index, expected and actual types). Exact types and int/float pairs
compare; date/datetime pairs do not. Names are literal. All written names count
toward 65,536 entries and 1,048,576 UTF-8 bytes, before early semantic returns.
Duplicate right fields and empty names are invalid metadata. Hosts still own
expression typing, authored diagnostic paths and current-schema compilation.

A validate_intermediate query has field and target. Target contains source
(kind dataset with name, or kind self with fields), derived, readable and
dependencies name lists. An empty readable list means unrestricted visibility.
A validate_intermediate_read query has read, with reader, target_name,
optional/null target, field, donor_dataset and visible names. Findings preserve
unknown_field before unavailable_dependency (with its zero-based dependency index).
Reading another SELF target returns only self_phase; self-reads defer to cycle
analysis and unavailable targets add no finding. The core uses literal names,
without record access, and hosts retain diagnostic provenance.

Intermediate context limits count all written entries, including duplicates and
skipped contexts: 65,536 names and 1,048,576 UTF-8 bytes. Limits remain separate
outcomes. Discover this additive query set via the intermediate_validation feature.

The envelope limit is 1,048,576 UTF-8 bytes. Core policies allow 4,096 output
columns, 256 datasets, 65,536 stored fields, 1,048,576 catalog-name bytes and
65,536 bytes per reference; a batch has at most 4,096 queries. Duplicated catalog
names and invalid context indices are metadata errors. Limits are separate
outcomes with exact decimal count strings. Declared Latin-1 is decoded explicitly;
byte-marked text and malformed UTF-8 are rejected without replacement. Each call
owns its catalog for the batch and retains no R objects afterwards.}
\examples{
analyze_references(paste0(
  '{"protocol":"reference-analysis/1","catalog":{"outputs":[],"datasets":[]},',
  '"queries":[{"kind":"bind","name":"UNKNOWN"}]}'
))
}
