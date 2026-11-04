\name{yamaa_cli}
\alias{yamaa_cli}
\title{Run one command of the yamaa command line as the R package}
\description{
Runs one command of the \code{yamaa} command line over this package's engine,
with the project functions installed in this R, and prints what the command
line prints on R's standard output and standard error. A run of a
specification that calls a project function, which the standalone binary
refuses, verifies the project's lock and calls the function as
\code{yamaa_project()$run()} does.
}
\usage{
yamaa_cli(arguments)
}
\arguments{
  \item{arguments}{A character vector: the words after \code{yamaa} as the
    command line takes them, such as
    \code{c("run", "project.yaml", "--spec", "ADSL")}. From \code{Rscript},
    \code{commandArgs(trailingOnly = TRUE)} gives them.}
}
\details{
The command line's \code{--host} and \code{--interpreter} flags settle the
project functions of the standalone binary, so this entry refuses them as a
usage error: the package runs its own. \code{--version} and \code{selftest}
name the host \code{r} and this R's version. Under \code{--verbose}, each
progress line reaches standard error as it happens. An interrupt is
signalled as every call of the package signals it.
}
\value{
The command line's exit status, invisibly: \code{0L} when the command
succeeded, \code{1L} when it completed with findings, and \code{2L} when it
could not run as asked. An \code{arguments} that is no character vector, or
holds a missing value, raises a condition of class \code{yamaa_error}.
}
\examples{
status <- yamaa_cli("--version")
status
\dontrun{
# From a shell, with the command line's exit status:
# Rscript -e 'quit(status = yamaa::yamaa_cli(commandArgs(TRUE)))' run project.yaml
}
}
