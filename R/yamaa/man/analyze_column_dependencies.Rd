\name{analyze_column_dependencies}
\alias{analyze_column_dependencies}
\title{Analyze bound column dependency rules through shared Rust}
\usage{analyze_column_dependencies(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON with ordered diagnostics and a column schedule, or
an explicit resource limit. Invalid transport raises an error after Rust returns.}
\description{Analyzes column-dependencies/1 metadata through the shared core.
Dependencies are indexed by the full output declaration order. A null entry has
no column-phase derivation; an empty array has a derivation with no output
references. Keys are zero-based indices in authored key order. has_rows is a
Boolean indicating row templates. Source references must be omitted by the binder.
This call does not bind names, select phases, compile a specification, read data
or invoke callbacks. It does not require Python.}
\details{Requests are limited to 1,048,576 UTF-8 bytes, 4,096 declarations and
65,536 written edges. Unknown or duplicate envelope fields, invalid indices and
duplicate keys are rejected. Repeated edges count against admission and retain
written diagnostic order. A complete outcome contains order and diagnostics.
Each diagnostic contains condition, requirement, location and columns. The
locations operation, expression and declaration tell hosts which authored paths
to attach. A cycle precedes forward-reference conditions, followed by key
conditions in authored key order. Diagnostics prevent execution even when order
contains schedulable columns. Completed row-phase values do not delay scheduling.
This is a bounded compiler service, not current-schema R execution support.
Declared Latin-1 is decoded explicitly; byte-marked text and malformed UTF-8
are rejected without locale fallback or replacement.}
\examples{
analyze_column_dependencies(
  '{"protocol":"column-dependencies/1","dependencies":[[]],"keys":[0],"has_rows":false}'
)
}
