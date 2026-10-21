\name{yamaa_info}
\alias{yamaa_info}
\title{What the engine is and where it runs}
\description{
Returns the facts that identify the installed engine: its version, the
language versions it reads, the features it was built with, the host and the
platform. A selftest's \code{info} holds the same record, and the command line
prints it for \code{yamaa --version}. It reads and writes no file.
}
\usage{
yamaa_info()
}
\value{
A named list, in this order:
\describe{
  \item{\code{version}}{The engine version, the same in every host; a release
    candidate reads like \code{"1.0.0-rc.1"}.}
  \item{\code{language_versions}}{Every \code{yamaa:} language version the
    engine reads, oldest first.}
  \item{\code{features}}{The optional features the engine was built with,
    sorted, among \code{"odm"}, \code{"parquet"}, \code{"readstat"} and
    \code{"yaml"}; every released package has all four.}
  \item{\code{host}}{\code{"r"}.}
  \item{\code{host_version}}{The version of R, as \code{getRversion()} gives
    it.}
  \item{\code{platform}}{The operating system and architecture of the engine,
    such as \code{"linux-x86_64"}.}
}
\code{packageVersion("yamaa")} stays the version of the package, which keeps
\code{1.0.0} while the release candidates carry their own engine version.
}
\examples{
info <- yamaa_info()
info$version
info$platform
}
