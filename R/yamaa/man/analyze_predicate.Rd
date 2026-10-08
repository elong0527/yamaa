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
compilation policies apply to each literal. All literals in one predicate also
share capture-dependent width-analysis ceilings of 1,000,000 work units and
1,000,000 logical storage cells. Resource refusals are not grammar errors.
Grammar outcomes also carry a production-owned message without a location suffix.
For unexpected characters the original position identifies the offending scalar.
Invalid regex literals report invalid_predicate under REQ-1244; invalid ESCAPE
literals report REQ-0191. The committed grammar's invalid_predicate result for
rejected temporal literals is retained, including typed temporal context; its
inconsistency with REQ-0192 remains unresolved. This syntax service itself does not evaluate data. The separately advertised
predicate_regex dataset capability executes admitted typed predicates. A full
R specification compiler remains outside this API.}
\examples{
analyze_predicate('{"protocol":"predicate-syntax/1","expression":"A IS NULL"}')
}
