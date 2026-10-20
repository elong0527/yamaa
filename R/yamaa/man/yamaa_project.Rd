\name{yamaa_project}
\alias{yamaa_project}
\alias{[[.yamaa_project_run}
\title{A project file, read only when a method runs}
\description{
Holds the path of a \code{project.yaml}: the study's specifications, codelists,
submission metadata and per-language environment files. Nothing is read until
a method runs.
}
\usage{
yamaa_project(path)

\method{[[}{yamaa_project_run}(x, i, ...)
}
\arguments{
  \item{path}{Path of a \code{project.yaml}.}
  \item{x}{A project run.}
  \item{i}{The \code{domain} name of a specification the run built.}
  \item{...}{Unused.}
}
\value{
An object with four methods:
\describe{
  \item{\code{$spec(name)}}{The listed specification whose \code{domain} is
    \code{name}, with the project's codelists and functions.}
  \item{\code{$check()}}{Static findings for every listed specification.}
  \item{\code{$run(section = NULL)}}{Builds every specification of one section
    (\code{"sdtm"}, \code{"adam"} or \code{"send"}), or of all sections. The
    project run has \code{$names}, \code{$issues}, \code{$report} and
    \code{$save()}, which writes each declared report file and then each output
    dataset in run order, only the reports when a specification failed;
    \code{runs[["ADSL"]]} is one specification's run, with \code{$output},
    \code{$issues} and \code{$report}.}
  \item{\code{$submission(section = NULL, created = NULL)}}{define.xml, and
    a Dataset-JSON or SAS Transport version 5 file per dataset where a section
    sets \code{datasets: json} or \code{datasets: xpt}, with \code{$issues}
    and \code{$save()}, which writes each section's transport files and then
    its define.xml.}
}
Each \code{$save()} returns the absolute paths it wrote invisibly, in
publication order. A failure raises a condition of class \code{yamaa_error}
whose \code{issues} data frame says why; for a refused \code{$save()}, its
\code{paths} lists the files written before the refusal.
}
\examples{
\dontrun{
project <- yamaa_project("project.yaml")
project$run("sdtm")$save()
project$submission("sdtm")$save()
}
}
