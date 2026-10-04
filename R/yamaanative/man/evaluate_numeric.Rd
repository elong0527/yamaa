\name{evaluate_numeric}
\alias{evaluate_numeric}
\title{Evaluate one bounded numeric request through the shared Rust engine}
\usage{evaluate_numeric(request)}
\arguments{\item{request}{One nonmissing UTF-8 JSON character string.}}
\value{Owned JSON describing a value, language failure, unsupported function or
compile resource limit. Malformed transport raises an R error after Rust returns.}
\description{Executes a normalized numeric/1 request using shared compilation and
completed-result conversion with optional literal handling. This prototype does
not execute specifications, tables, project callbacks or output publication,
and never falls back to another evaluator.}
\details{Inputs use the scalar/1 lossless value representation. Requests are capped
at 1,048,576 UTF-8 bytes and 4,096 bindings before execution. Parser/compiler budgets
also apply. The default reference_subset policy rejects EXP, LN and POWER;
portable_libm_v1 is an explicit numerical opt-in. See the package README for the
request and result schemas and remaining qualification gates.}
\examples{
evaluate_numeric(paste0(
  '{"protocol":"numeric/1","expression":"A + 1","column_path":"columns.A",',
  '"target":"int","bindings":[{"name":"A","value":{"int":"7"}}]}'
))
}
