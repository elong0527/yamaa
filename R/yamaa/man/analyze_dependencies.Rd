\name{analyze_dependencies}
\alias{analyze_dependencies}
\title{Analyze one already-bound dependency graph through shared Rust}
\usage{analyze_dependencies(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON with a cycle path and stable order, or an explicit
resource limit. Invalid transport raises an R error after Rust returns.}
\description{Analyzes a dependency-analysis/1 graph through the shared core.
Nodes are zero-based declaration indices. Each dependencies entry lists the
nodes read by that declaration. This call neither binds names nor compiles
specifications, reads data or invokes callbacks. It does not require Python.}
\details{Requests are limited to 1,048,576 UTF-8 bytes, 4,096 nodes and 65,536
written edges. Duplicate edges count against the limit but do not change the
graph result. A complete outcome contains cycle (null or a closed cycle path)
and order. The first cycle found by declaration-ordered depth-first traversal
is rotated to its earliest declared member. Scheduling repeatedly selects the
earliest ready declaration; cycle-blocked nodes are omitted. Forward-reference
and phase legality must be checked by a compiler before using this order.
Declared Latin-1 is decoded explicitly; byte-marked text and malformed UTF-8
are rejected without locale fallback or replacement.}
\examples{
analyze_dependencies(
  '{"protocol":"dependency-analysis/1","dependencies":[[1],[]]}'
)
}
