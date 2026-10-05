\name{analyze_aggregate}
\alias{analyze_aggregate}
\title{Parse aggregate syntax through shared Rust}
\usage{analyze_aggregate(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON. Malformed transport raises an R error after Rust
returns; grammar failures and resource limits are data outcomes.}
\description{Parses an aggregate-syntax/1 expression without binding names,
reading data or granting execution support. Python is not involved. The AST
retains number spelling and original reducer text. Ordered identifiers,
record-star relations and ungrouped identifiers remain separate.}
\details{The request is limited to 1,048,576 UTF-8 bytes. Parser budgets are
65,536 expression bytes, 8,192 tokens, 4,096 nodes and depth 64. Resource limits
are policy outcomes, not language conditions. Grammar errors retain requirement,
context and zero-based UTF-8 byte and Unicode scalar character offsets. All
six reducers and imported numeric syntax parse independently of evaluator
support. The current specification frontend remains a separate compiler gate.}
\examples{
analyze_aggregate('{"protocol":"aggregate-syntax/1","expression":"COUNT(D.*)"}')
}
