\name{yamaa_resolve_specification}
\alias{yamaa_resolve_specification}
\alias{yamaa_read_resolved_specification}
\alias{as.character.yamaa_resolved_specification}
\title{Versioned Resolved Specification Inspection Facts}
\usage{
yamaa_resolve_specification(specification, environment = NULL)
yamaa_read_resolved_specification(json)
\method{as.character}{yamaa_resolved_specification}(x, ...)
}
\arguments{
\item{specification}{One original domain specification path.}
\item{environment}{Optional study environment path.}
\item{json}{One character scalar containing the distinct resolved JSON format.}
\item{x}{An owned resolved specification artifact.}
\item{...}{Unused arguments.}
}
\value{An owned JSON character scalar of class \code{yamaa_resolved_specification}.
Use \code{as.character()} to export its exact JSON or an optional JSON reader to
inspect fields. Resolution failures signal \code{yamaa_resolved_specification_error}
with an \code{issues} data frame and the same findings as \code{diagnostics}
records (see \code{\link{yamaa_domain}}). Invalid serialized artifacts signal an error.}
\description{Prepares original specifications through the shared engine and retains
the composed normalized model, original field provenance, resource locations and
producer-specific entry identities. It reads metadata without invoking project code.}
\details{Artifact version 1 preserves named dictionary references. The distinct
format records lexical locations, not resource availability or file access
capabilities. Deserialization replays inspection facts only; it never opens
resources, resolves paths, invokes callbacks or compiles an executable program.
Source deletion and working-directory changes leave held facts unchanged.
Existing build and check APIs continue to accept original specification paths.}
