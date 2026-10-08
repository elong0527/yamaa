\name{invoke_function}
\alias{invoke_function}
\title{Invoke one explicit callback through the shared Rust engine}
\usage{invoke_function(request, callback)}
\arguments{
\item{request}{One unclassed, nonmissing function/1 JSON character string without attributes.}
\item{callback}{An explicitly supplied R function. No function-name lookup is performed.}
}
\value{Owned JSON containing a scalar value or a fatal condition with portable
identity, phase and diagnostic facts. Transport errors raise after native return.}
\description{Validates a bounded normalized function/1 request and invokes one
already-bound function synchronously on the R thread. Defaults, mapped argument
order, exact types, missing short-circuit and result checks use the shared engine.}
\details{Int arguments use lossless yamaa_int64 and str arguments use yamaa_utf8;
float and bool use native double and logical scalars, and missing is logical NA.
Temporal arguments use Date and explicitly UTC POSIXct. Collected temporal
precision drops at this host boundary; returns have day/second precision.
Results also admit unclassed R integer/character scalars. Unknown classes,
extra attributes, collections, fractional epochs and non-UTC datetimes are
rejected. NA/nonfinite scalars normalize to missing before nullable checks.

Ordinary callback errors and invalid return representations are distinct fatal
outcomes. Interrupts are re-signaled after Rust returns. Effects are not retried
or rolled back; nested calls are permitted. Requests and returned text have a
1 MiB limit; at most 256 parameters/arguments and 1,024 bytes per name are
admitted. Output is capped at 8 MiB and each host error detail at 8,192 UTF-8
bytes with explicit truncation. These limits do not bound callback execution
or its allocations. Caller labels do not verify artifact membership; environment
activation, specification execution and release qualification remain separate.}
\examples{
request <- paste0(
  '{"protocol":"function/1","identity":{"name":"example",',
  '"contract_version":"1","implementation_version":"1","call":"pkg::value"},',
  '"parameters":[],"arguments":[],"returns":"int","may_return_missing":false}'
)
invoke_function(request, function() int64("9007199254740993") + 1L)
}
