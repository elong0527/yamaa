\name{prepare_specification}
\alias{prepare_specification}
\alias{specification_source}
\alias{specification_failure_report}
\alias{specification_report}
\title{Compile and report bounded original YAML runs through Rust}
\usage{
prepare_specification(modules, entry, identity, source)
specification_source(handle)
specification_report(handle, capture, publish, example,
  specification = "spec.yaml", base_directory = ".", inspect = NULL)
specification_failure_report(handle, capture, example,
  specification = "spec.yaml", base_directory = ".", inspect = NULL)
}
\arguments{
\item{modules}{Named list of raw schema YAML snapshots.}
\item{entry}{Entry schema module name.}
\item{identity}{Original specification source identity.}
\item{source}{Raw original specification YAML.}
\item{handle}{Owned prepared specification handle.}
\item{capture}{Explicit callback taking name, path and maximum bytes, returning a list of raw bytes and a logical newly-created-snapshot flag. A known resource failure may return a list of \code{"missing"} or \code{"not_regular_file"} and the original condition. Rust owns its diagnostic and failed report. Thrown errors and interruptions retain their original identity.}
\item{inspect}{Optional metadata-only callback taking source name and path. Return NULL for an admitted regular file or an explicit resource-failure reply. Every declared source is inspected before capture; classified failures collect in declaration order without study reads.}
\item{publish}{Callback taking the authored output path and raw rendered bytes. It owns path authorization and atomic replacement, and must return \code{TRUE} only after publication completes. Any other return value rejects publication.}
\item{example}{Example identity in the report.}
\item{specification}{Relative specification identity in observations.}
\item{base_directory}{Relative source-base identity in observations.}
}
\value{An owned handle, source request, or portable report JSON.}
\description{
The current shared compiler admits one CSV driver with declared scalar types,
source/compute columns, and a closed row-template/ordered-SUM slice. Other
semantics are explicitly rejected. The source callback owns
filesystem authorization, bounded immutable capture, caching and verification;
Rust owns YAML interpretation, binding, execution and observations. Original R
errors and interruptions are rethrown after Rust returns. The report entrypoint validates and renders the
complete result before invoking the publication callback. Failed runs publish no
primary output. The failure-report entrypoint remains available without a
publisher. Inheritance and the full language remain unsupported, and these APIs
do not change the default runtime. No Python process or R semantic planner
is used. Raw-byte dataset execution remains an internal integration API.
}
