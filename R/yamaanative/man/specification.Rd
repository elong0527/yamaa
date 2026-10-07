\name{prepare_specification}
\alias{prepare_specification}
\alias{specification_source}
\alias{specification_failure_report}
\title{Compile original YAML and observe bounded failure runs through Rust}
\usage{
prepare_specification(modules, entry, identity, source)
specification_source(handle)
specification_failure_report(handle, capture, example,
  specification = "spec.yaml", base_directory = ".")
}
\arguments{
\item{modules}{Named list of raw schema YAML snapshots.}
\item{entry}{Entry schema module name.}
\item{identity}{Original specification source identity.}
\item{source}{Raw original specification YAML.}
\item{handle}{Owned prepared specification handle.}
\item{capture}{Explicit callback taking name, path and maximum bytes, returning a list of raw bytes and a logical newly-created-snapshot flag.}
\item{example}{Example identity in the report.}
\item{specification}{Relative specification identity in observations.}
\item{base_directory}{Relative source-base identity in observations.}
}
\value{An owned handle, source request, or complete portable failure-report JSON respectively.}
\description{
The current shared compiler admits one untyped CSV driver with source/compute
columns. Other semantics are explicitly rejected. The source callback owns
filesystem authorization, bounded immutable capture, caching and verification;
Rust owns YAML interpretation, binding, execution and observations. Original R
errors and interruptions are rethrown after Rust returns. Failure reporting does
not implement successful artifact publication, inheritance or the full language,
and does not change the default runtime. No Python process or R semantic planner
is used. Raw-byte dataset execution remains an internal integration API.
}
