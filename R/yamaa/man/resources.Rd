\name{yamaa_resource_resolver}
\alias{yamaa_resource_resolver}
\alias{yamaa_resource_result}
\alias{yamaa_resource_failure}
\alias{yamaa_memory_resolver}
\alias{yamaa_synthetic_remote_resolver}
\title{Host Resource Resolution and Publication}
\usage{
yamaa_resource_resolver(callback)
yamaa_resource_result(reference, data = NULL, frame = NULL)
yamaa_resource_failure(reason)
yamaa_memory_resolver(objects = list(), prefix = "memory:/")
yamaa_synthetic_remote_resolver(objects = list())
}
\arguments{
\item{callback}{Function taking a typed request list and returning a resource result.}
\item{reference}{Immutable opaque object identity, excluding credentials.}
\item{data}{Raw captured bytes, or NULL outside capture.}
\item{frame}{Immutable Arrow-compatible frame, or NULL. Supported public physical types are UTF-8, signed 64-bit integers, doubles, dates and timestamps in whole seconds without a timezone.}
\item{reason}{One of missing, changed, cancelled, rejected, publication or cleanup.}
\item{objects}{Named list of raw input objects.}
\item{prefix}{Opaque identity prefix.}
}
\value{A resolver environment, a typed resource result, or a signalled classified failure.}
\description{Supply a resolver to \code{yamaa_domain} or \code{yamaa_check} to map local declaration spellings to local, URI or memory identities without creating temporary input files.}
\details{Requests contain operation, kind, declaring, written, reference and data.
The initial declaring location is \code{/}. Later declaring locations are logical absolute paths; returned backend references are never interpreted as paths. Inspect and target return references without reading study data. Capture returns bytes or an immutable frame. Repeated captures in one attempt must keep the reference and exact held content. New attempts accept new versions. Provenance keeps original declaring locations and explicit references, separately from credentials. No content hashes or digest fields are used.
Stage returns a provisional token. Commit atomically publishes that token. Abort removes it after failure or interruption, including a stage failure that only identifies the target. Save uses retained bytes, publishes the report file before primary output, and preserves the completed prefix if a later artifact fails. Source bytes are never reopened at Save.
The memory resolver exposes its input and output state in \code{resolver$backend}; use \code{backend$put(key, data, reference, frame)} to replace an object version. Frames must remain immutable while retained. Conditions and interrupts retain their original R objects through cleanup and are signalled after the native boundary returns.}
