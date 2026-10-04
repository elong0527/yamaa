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
\details{The plan is admitted before source IPC decoding. Record/group templates, direct/literal/completed-column values, ordered SUM/MEAN, key checks and the documented verification subset run in the shared Rust engine. Request, shape, retained-text, scalar-text-work, identity and response limits are fixed native policies. JSON integers and counts retain decimal spelling and floats retain exact bits. Only success exposes table bytes. Transport errors raise after the native call returns. Requests use the package's checked UTF-8 policy, including explicit Latin-1 decoding. There is no file access, callback activation, fallback or artifact publication. Execution is bounded and synchronous; asynchronous cancellation and the complete workflow/report interfaces remain separate qualification gates.}
