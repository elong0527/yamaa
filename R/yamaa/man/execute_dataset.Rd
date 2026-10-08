\name{execute_dataset}
\alias{execute_dataset}
\title{Execute an explicitly bound dataset plan through Rust}
\usage{execute_dataset(request, source)}
\arguments{
\item{request}{One unclassed, nonmissing JSON character string.}
\item{source}{A raw vector containing one canonical source IPC snapshot.}
}
\value{A list containing \code{table}, owned raw IPC on success or NULL on failure, and \code{outcome}, exact JSON observations.}
\description{Runs the closed dataset/1 typed-plan bridge on the calling R thread. This optional API is not a specification compiler or default backend.}
\details{The plan is admitted before source IPC decoding. Record/group templates, direct/literal/completed-column values, ordered SUM/MEAN, key checks and the documented verification subset run in the shared Rust engine. Literal-pattern Contains predicates search through the shared Rust regex core; patterns compile before IPC decoding. Matching shares limits across nodes, rows and check declarations: each evaluation permits 1,048,576 subject bytes and 1,000,000 work/state cells; a dataset attempt permits 16 MiB subject bytes and 4,194,304 work/state cells. These matching counters are independent of other work/text counters and resource refusals are not false or unknown predicate values. Request, shape, retained-text, scalar-text-work, identity and response limits are fixed native policies. JSON integers and counts retain decimal spelling and floats retain exact bits. Only success exposes table bytes. Transport errors raise after the native call returns. Requests use the package's checked UTF-8 policy, including explicit Latin-1 decoding. There is no file access, callback activation, fallback or artifact publication. Execution is bounded and synchronous; asynchronous cancellation and the complete workflow/report interfaces remain separate qualification gates.}
