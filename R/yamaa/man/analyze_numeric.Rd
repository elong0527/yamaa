\name{analyze_numeric}
\alias{analyze_numeric}
\title{Parse numeric syntax through shared Rust}
\usage{analyze_numeric(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON. Malformed transport raises an R error after Rust
returns; grammar failures and resource limits are data outcomes.}
\description{Parses a numeric-syntax/1 expression without binding names,
reading data or granting execution support. Python is not involved. The AST
retains exact number spelling and ordered distinct identifiers.}
\details{The request is limited to 1,048,576 UTF-8 bytes. Parser budgets are
65,536 expression bytes, 8,192 tokens, 4,096 nodes and depth 64. Resource limits
are policy outcomes, not language conditions. Grammar errors retain requirement,
context and zero-based UTF-8 byte and Unicode scalar character offsets. Every declared numeric function parses independently of evaluator support. The current specification frontend remains a separate compiler gate.}
\examples{
analyze_numeric('{"protocol":"numeric-syntax/1","expression":"ABS(D.V)"}')
}
