\name{yamaa_define}
\alias{yamaa_define}
\title{Generate submission documents through the shared Rust engine}
\usage{yamaa_define(environment, standard, created = NULL)}
\arguments{
\item{environment}{One environment YAML path.}
\item{standard}{One of sdtm, adam or send.}
\item{created}{Optional whole-second civil datetime; omission uses current UTC.}
}
\value{An owned result with issues, diagnostics and a save method. The issues
data frame and the diagnostics record list are built natively from the same
findings, with the record shape described in \code{\link{yamaa_domain}}. A failed
save signals \code{yamaa_define_error} carrying both views.}
\description{Composes submission metadata and holds deterministic Define-XML and,
when the section requests a transport, SAS Transport version 5 or Dataset-JSON
bytes until Save. No specification is executed.}
