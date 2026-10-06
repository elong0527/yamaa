\name{analyze_predicate}
\alias{analyze_predicate}
\title{Parse predicate syntax through shared Rust}
\usage{analyze_predicate(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON. Malformed transport raises an R error after Rust
returns; grammar failures and resource limits are data outcomes.}
\description{Parses a predicate-syntax/1 expression without binding names,
reading data or granting execution support. Python is not involved. The AST
retains number spelling, decoded string literals, character positions and ordered
unique identifiers. Regex literals are admitted by the shared Rust regex core.}
\details{The request is limited to 1,048,576 UTF-8 bytes. Parser policies are
65,536 expression bytes, 8,192 tokens, 4,096 arena nodes and depth 64.
Whole-input tokenization precedes syntax and regex admission. Per-pattern regex
compilation policies apply separately. Resource refusals are not grammar errors.
Invalid regex literals report invalid_predicate under REQ-1244; invalid ESCAPE
literals report REQ-0191. The committed grammar's invalid_predicate result for
rejected temporal literals is retained, including typed temporal context; its
inconsistency with REQ-0192 remains unresolved. This service does not enable
regex predicate execution or replace the specification frontend.}
\examples{
analyze_predicate('{"protocol":"predicate-syntax/1","expression":"A IS NULL"}')
}
