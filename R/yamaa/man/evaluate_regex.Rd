\name{evaluate_regex}
\alias{evaluate_regex}
\title{Compile or match a portable regular expression through Rust}
\usage{evaluate_regex(request)}
\arguments{
\item{request}{One unclassed, nonmissing JSON character string without attributes.}
}
\value{Owned outcome JSON. Malformed transport raises an R error only after the
native call returns. Requests and serialized responses are bounded.}
\description{Accepts regex/1 JSON with a pattern and a compile, search or full_match
operation. Compile-only requests contain no subject. Captures distinguish empty
text, an unentered group and no match. Invalid patterns and resource refusals are
structured outcomes. This opt-in service grants no dataset execution capability
and uses no Python or host regex library.}
\examples{
evaluate_regex('{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile"}}')
}
