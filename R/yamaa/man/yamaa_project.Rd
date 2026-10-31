\name{yamaa_project}
\alias{yamaa_project}
\alias{[[.yamaa_project_run}
\alias{print.yamaa_project}
\alias{format.yamaa_project}
\alias{print.yamaa_project_run}
\alias{format.yamaa_project_run}
\alias{names.yamaa_project_run}
\alias{length.yamaa_project_run}
\alias{print.yamaa_member_run}
\alias{format.yamaa_member_run}
\alias{names.yamaa_member_run}
\alias{length.yamaa_member_run}
\alias{print.yamaa_submission}
\alias{format.yamaa_submission}
\alias{names.yamaa_submission}
\alias{length.yamaa_submission}
\title{A project file, read only when a method runs}
\description{
Holds the path of a \code{project.yaml}: the study's specifications, codelists,
submission metadata, function files and per-language locks. Nothing is read
until a method runs.
}
\usage{
yamaa_project(path)

\method{[[}{yamaa_project_run}(x, i, ...)

\method{print}{yamaa_project}(x, ...)

\method{format}{yamaa_project}(x, ...)

\method{print}{yamaa_project_run}(x, ...)

\method{format}{yamaa_project_run}(x, ...)

\method{names}{yamaa_project_run}(x)

\method{length}{yamaa_project_run}(x)

\method{print}{yamaa_member_run}(x, ...)

\method{format}{yamaa_member_run}(x, ...)

\method{names}{yamaa_member_run}(x)

\method{length}{yamaa_member_run}(x)

\method{print}{yamaa_submission}(x, ...)

\method{format}{yamaa_submission}(x, ...)

\method{names}{yamaa_submission}(x)

\method{length}{yamaa_submission}(x)
}
\arguments{
  \item{path}{Path of a \code{project.yaml}, taken as \code{\link{yamaa_spec}}
    takes its path: names and a class are dropped, and a leading \code{~}
    expands to the home directory.}
  \item{x}{A project, a project run, one specification's run of a project run,
    or a submission.}
  \item{i}{The \code{domain} name of a specification the run built.}
  \item{...}{Unused.}
}
\value{
An object with a read-only field and four methods:
\describe{
  \item{\code{$names}}{The \code{domain} names of the listed specifications,
    in the order that a run of every section builds them, each after the ones
    it reads from, read without study data. A project file that cannot be read,
    or that lists a name twice, raises.}
  \item{\code{$spec(name)}}{The listed specification whose \code{domain} is
    \code{name}, with the project's codelists and functions.}
  \item{\code{$check()}}{Static findings for every listed specification, with
    \code{$status} and \code{$issues}.}
  \item{\code{$run(section = NULL)}}{Builds every specification of one section
    (\code{"sdtm"}, \code{"adam"} or \code{"send"}), or of all sections. The
    project run has \code{$status}, \code{"succeeded"} when every
    specification succeeded, \code{$names}, \code{$issues}, \code{$report} and
    \code{$save()}, which writes each declared report file and then each output
    dataset in run order, only the reports when a specification failed;
    \code{runs[["ADSL"]]} is one specification's run, with \code{$status},
    \code{$output}, \code{$issues} and \code{$report}. Its status is
    \code{"skipped"} when a specification that it reads from did not
    complete.}
  \item{\code{$submission(section = NULL, created = NULL)}}{define.xml, and
    a Dataset-JSON or SAS Transport version 5 file per dataset where a section
    sets \code{datasets: json} or \code{datasets: xpt}, with \code{$status},
    \code{$created}, the ISO 8601 creation time the documents carry (the
    \code{created} given, or else the time read once for every section),
    \code{$issues} and \code{$save()}, which writes each section's transport
    files and then its define.xml.}
}
\code{$check()}, \code{$run()} and \code{$submission()} also take
\code{progress}, a function called with one named list per event, as
\code{\link{yamaa_spec}} describes: each specification of a check or run is
named by its \code{domain}, and each section of a submission is one
\code{"composition"} phase. A \code{progress} function that signals an error
is called no more: the call checks, builds or composes nothing further, and
its result fails with one \code{engine_rejected} issue whose code is
\code{host_progress_failure} and refuses its save. A check, a project run,
one specification's run and a submission hold \code{$timings}, a data frame
of each phase the call ran; no saved file holds them. The three methods also
take \code{threads}, the most partitions of one run that derive at once, as
\code{\link{yamaa_spec}} describes.

Each \code{$save()} returns the absolute paths it wrote invisibly, in
publication order. A failure raises a condition of class \code{yamaa_error}
whose \code{issues} data frame says why; for a refused \code{$save()}, its
\code{paths} lists the files written before the refusal.

\code{format()} returns the lines that the engine renders for an object, the
same words that Python and the command line print, and \code{print()} writes
them and returns the object invisibly. A project prints
\code{yamaa project <path>}, since it has read nothing. A project run prints
its status, its number of specifications and its issue count, then one line
per specification with its status, size and issue count, then its first
issue; one specification's run prints as a run does. A submission prints its
status, its define.xml and transport files and its issue count.

\code{names()} of a project run gives the names of its specifications, as
\code{$names} does, and \code{length()} counts them. \code{names()} of one
specification's run or of a submission lists its fields and methods in the
order above, starting with \code{status}, and \code{length()} counts them;
\code{ls()} lists the same names, sorted.
}
\examples{
study <- tempfile("yamaa-")
dir.create(study)
writeLines(c("USUBJID,AGE", "01,35", "02,42"), file.path(study, "input.csv"))
writeLines('yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [USUBJID]
input: {RAW: {path: input.csv, types: {USUBJID: str, AGE: int}}}
output: {path: dm.csv, columns: [USUBJID, AGE]}

columns:
  - {name: USUBJID, type: str, derivation: RAW.USUBJID}
  - {name: AGE, type: int, derivation: RAW.AGE}
', file.path(study, "dm.yaml"))
writeLines('yamaa: "1.0"
type: project
sdtm: {specs: [dm.yaml]}
', file.path(study, "project.yaml"))

project <- yamaa_project(file.path(study, "project.yaml"))
project$names
project$check()
runs <- project$run("sdtm")
runs
runs[["DM"]]$output
basename(runs$save())

unlink(study, recursive = TRUE)
}
