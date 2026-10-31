\name{yamaa_spec}
\alias{yamaa_spec}
\alias{print.yamaa_spec}
\alias{format.yamaa_spec}
\alias{print.yamaa_check}
\alias{format.yamaa_check}
\alias{names.yamaa_check}
\alias{length.yamaa_check}
\alias{print.yamaa_run}
\alias{format.yamaa_run}
\alias{names.yamaa_run}
\alias{length.yamaa_run}
\title{One specification, read only when a method runs}
\description{
Holds the path of one specification. Nothing is read until a method runs, and
each method rereads the files. The run method accepts \code{inputs = list(DM = df)}
to replace declared input aliases with data frames. Unspecified inputs keep
reading files; declared types still apply. Values are copied for eager and
streamed runs, and reports name memory inputs as \code{memory:<alias>}.
Characters and factors, integers, numerics, Date and whole-second POSIXct values
are supported; an R Arrow package is not needed. Project runs keep file inputs.
}
\usage{
yamaa_spec(path)

\method{print}{yamaa_spec}(x, ...)

\method{format}{yamaa_spec}(x, ...)

\method{print}{yamaa_check}(x, ...)

\method{format}{yamaa_check}(x, ...)

\method{names}{yamaa_check}(x)

\method{length}{yamaa_check}(x)

\method{print}{yamaa_run}(x, ...)

\method{format}{yamaa_run}(x, ...)

\method{names}{yamaa_run}(x)

\method{length}{yamaa_run}(x)
}
\arguments{
  \item{path}{Path of a specification file: one character string, which may
    carry names or a class, as an \code{fs::path()} or \code{glue::glue()}
    value does. A leading \code{~} expands to the home directory, as
    \code{\link[base]{path.expand}} expands it. The \code{parent} of
    \code{$check()} is taken the same way.}
  \item{x}{A specification, a check or a run.}
  \item{...}{Unused.}
}
\value{
An object with three methods:
\describe{
  \item{\code{$check(parent = NULL)}}{A check: \code{$status} is
    \code{"failed"} when the check found a problem, and \code{"succeeded"}
    otherwise, and the \code{$issues} data frame lists the problems found
    without reading study data or running project code. With \code{parent},
    the path of another specification, the specification is checked as a
    fragment whose parent is \code{parent}, as it would compose in a file
    that names \code{parent} as its \code{parent}, includes the fragment and
    writes nothing else; a specification that declares \code{parent} or
    \code{include} is no fragment, and the call fails with a
    \code{yamaa_error} condition.}
  \item{\code{$resolve()}}{The specification after its layers are composed, as
    a named list.}
  \item{\code{$run()}}{A run: \code{$status} is \code{"succeeded"} when the run
    built its dataset with no issue, and \code{"failed"} otherwise,
    \code{$output} is the dataset (\code{NULL} when the run failed),
    \code{$issues} what is wrong, \code{$report} what the run read, counted
    and checked, and \code{$save()} writes the declared report file, then the
    output dataset, and returns their absolute paths invisibly, in that order.
    A failed run writes only its report file.

    \code{$run(stream = TRUE)} builds the dataset as its output is read, and
    reads nothing until then: \code{$output} is an Arrow C stream, an
    external pointer of class \code{nanoarrow_array_stream}, which nanoarrow
    and arrow read, one batch per pull; the first pull builds the run. The
    other fields give the run once its stream has ended, and run it whole
    when nothing has read the output, whose stream then reads the dataset it
    keeps. Releasing the stream before its last batch cancels the run, which
    then saves nothing.}
}
\code{$check()} and \code{$run()} also take \code{progress}, a function
called on the calling thread with one named list per event as each phase
starts and ends: \code{name}, \code{phase}, \code{kind} (\code{"start"},
\code{"end"}, \code{"partition"} or \code{"batch"}), \code{outcome},
\code{number} and \code{rows}, the last three \code{NULL} where the event has
none. A \code{progress} function that signals an error is called no more,
and the check or run then fails with one \code{engine_rejected} issue whose
code is \code{host_progress_failure}; such a run saves nothing. A check and
a run hold \code{$timings}, a data frame of each phase the call ran, with the
columns \code{name}, \code{phase}, \code{started_ms}, \code{elapsed_ms} and
\code{outcome}; no saved file holds them.

Both also take \code{threads}, the most partitions of one run that derive at
once, each on a thread of its own: one positive whole number, or
\code{NULL}, the default, for the fewer of 4 and the cores the process may
use. \code{threads = 1} derives one partition at a time. The result, its
issues, its report and the progress events are the same for every value;
only the time and the peak memory change. Any other value fails the call
with a \code{yamaa_error} condition.

A failure raises a condition of class \code{yamaa_error} whose \code{issues}
data frame says why; for a refused \code{$save()}, its \code{paths} lists
the files written before the refusal.

Each row of an \code{issues} data frame is one problem, and its
\code{context} column holds the problem's facts as canonical JSON text, one
object per row. \code{jsonlite::fromJSON()} parses it into a named list, so
\code{lapply(issues$context, jsonlite::fromJSON, bigint_as_char = TRUE)}
gives one list per issue, keeping an integer beyond 2^53 as text;
jsonlite is a suggested package, which you install yourself. The column is
text so that the package needs no JSON library, and because an R double
cannot hold every integer that a context may hold.

\code{format()} returns the lines that the engine renders for an object, the
same words that Python and the command line print, and \code{print()} writes
them and returns the object invisibly. A specification prints
\code{yamaa spec <path>}, with the path as given and its \code{~} expanded,
since it has read nothing.
A check prints \code{yamaa check of <path>: <status>, <n> issues}, and a run
\code{yamaa run of <dataset>: <status>, <size>, <n> issues}, where the size
is \code{<r> rows x <c> columns} or \code{no output}; either then prints its
first issue on a line of its own.

\code{names()} of a check or a run lists its fields and methods in the order
above, starting with \code{status}, and \code{length()} counts them;
\code{ls()} lists the same names, sorted.
}
\examples{
study <- tempfile("yamaa-")
dir.create(study)
writeLines(c("USUBJID,AGE", "01,35", "02,42"), file.path(study, "input.csv"))
writeLines('yamaa: "1.0"
type: spec
domain: DM
keys: [USUBJID]
input: {RAW: {path: input.csv, types: {USUBJID: str, AGE: int}}}
output: {path: dm.csv, columns: [USUBJID, AGE]}

columns:
  - {name: USUBJID, type: str, derivation: RAW.USUBJID}
  - {name: AGE, type: int, derivation: RAW.AGE}
', file.path(study, "dm.yaml"))

spec <- yamaa_spec(file.path(study, "dm.yaml"))
spec$check()
run <- spec$run()
run
run$output
basename(run$save())

unlink(study, recursive = TRUE)
}
