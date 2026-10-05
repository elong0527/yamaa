\name{execute_dataset_functions}
\alias{execute_dataset_functions}
\title{Execute a typed dataset with explicit callbacks through Rust}
\usage{execute_dataset_functions(request, source, secondary, callbacks)}
\arguments{
\item{request}{One unclassed, nonmissing JSON character string.}
\item{source}{A raw vector containing canonical source IPC.}
\item{secondary}{A list of raw secondary IPC vectors in declaration order.}
\item{callbacks}{A list of at most 64 explicitly supplied functions in declaration order.}
}
\value{A list with owned \code{table} IPC on success and exact \code{outcome} JSON. Transport failures and original interruptions raise after native return.}
\description{Composes explicitly supplied R callbacks with the bounded dataset/1 typed-plan bridge on the calling R thread. It does not compile specifications or activate environments.}
\details{Bindings are admitted before IPC decoding. Supplied argument reads preserve authored order; calls use declaration-order host arguments and the shared exact-type/default/missing lifecycle. Callback references are captured for the run. Fatal callback failures expose no accepted table, but earlier effects are not retried or rolled back. Python remains the default backend; this optional interface does not establish full engine readiness.}
