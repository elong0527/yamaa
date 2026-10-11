\name{yamaa_define}
\alias{yamaa_define}
\title{Generate submission documents through the shared Rust engine}
\usage{yamaa_define(environment, standard, created = NULL)}
\arguments{
\item{environment}{One environment YAML path.}
\item{standard}{One of sdtm, adam or send.}
\item{created}{Optional whole-second civil datetime; omission uses current UTC.}
}
\value{An owned result with issues and a save method.}
\description{Composes submission metadata and holds deterministic Define-XML and,
when requested, Dataset-JSON bytes until Save. No specification is executed.}
