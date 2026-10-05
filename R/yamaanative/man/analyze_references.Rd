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
row identity and authored paths. This does not check qualified-reference reach,
grouping, joins or the rest of the language, and is not a current-schema compiler.

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
