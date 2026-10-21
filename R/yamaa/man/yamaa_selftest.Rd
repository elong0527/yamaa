\name{yamaa_selftest}
\alias{yamaa_selftest}
\alias{print.yamaa_selftest}
\alias{format.yamaa_selftest}
\alias{names.yamaa_selftest}
\alias{length.yamaa_selftest}
\title{Run the bundled selftest cases through the installed engine}
\description{
Runs the selftest cases that the package carries, benchmarks with their
independently authored expected files, through the installed engine and
compares each result with its expected files. It reads and writes no file and
uses no network. Its report is the installation and operational
qualification evidence of an installation.
}
\usage{
yamaa_selftest()

\method{print}{yamaa_selftest}(x, ...)

\method{format}{yamaa_selftest}(x, ...)

\method{names}{yamaa_selftest}(x)

\method{length}{yamaa_selftest}(x)
}
\arguments{
  \item{x}{A selftest result.}
  \item{...}{Unused.}
}
\value{
An object with three read-only fields:
\describe{
  \item{\code{$status}}{\code{"failed"} when a case failed, and
    \code{"succeeded"} otherwise.}
  \item{\code{$cases}}{A data frame with one row per case and the columns
    \code{case}, \code{status} and \code{message}. A case's status is
    \code{"succeeded"}, \code{"failed"}, or \code{"skipped"} when this build
    does not compile a feature the case needs; its message says why it
    failed or was skipped, and is \code{NA} when it passed.}
  \item{\code{$info}}{A named list: the engine \code{version}, the
    \code{language_versions} it reads, its compiled \code{features}, the
    \code{host} (\code{"r"}), the \code{host_version} and the
    \code{platform}.}
}
\code{format()} returns the lines of the report, and \code{print()} writes
them and returns the result invisibly. The first line reads
\code{yamaa selftest: <status>, <p> of <c> cases passed}. The first failed
case and the adapters that no case covered follow, then the engine version,
the language versions, the host and the platform. \code{names()} lists the
three fields in that order, and \code{length()} counts them.
}
\examples{
result <- yamaa_selftest()
result
result$cases
}
