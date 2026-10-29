\name{yamaa_lint}
\alias{yamaa_lint}
\title{Lint yamaa files against the style guide}
\description{
Reports how each yamaa file departs from the written form the yamaa style
guide fixes: field order, blank lines, line width, the canonical spellings of
a 1.0 file and the authoring findings. A finding describes the text, never its
meaning, and a file runs as its canonical form does.
}
\usage{
yamaa_lint(paths, fix = FALSE)
}
\arguments{
  \item{paths}{Paths of yamaa files, a character vector whose names and class
    are dropped and whose leading \code{~} expands to the home directory, as
    \code{\link{yamaa_spec}} takes its path. A directory names the yamaa files
    of every file type beneath it, outside hidden directories.}
  \item{fix}{Whether to apply the proved fixes first. A file is replaced only
    when the fixed text reads as the same values with the same comments, or
    as those values with exactly the canonical rewrite a finding names.}
}
\details{
A comment \code{# yamaa-lint: allow <finding> -- <reason>} directly above a
field or an entry suppresses the named findings within it.
}
\value{
A read-only \code{yamaa_lint_result} with \code{status}, \code{findings} and
\code{paths}. Status is \code{failed} while a finding remains, and
\code{succeeded} otherwise. The paths are the absolute file names a fix
rewrote, as the command line JSON reports them. Printing gives the engine
summary. The findings data frame has one row per remaining finding and the columns
\code{file} (the absolute path, with forward slashes), \code{line},
\code{column}, \code{finding}, \code{spec_path} and \code{message}. A file
that cannot be read or parsed, or a directory that cannot be listed, raises a
condition of class \code{yamaa_error}, and nothing is written.
}
\examples{
study <- tempfile("yamaa-")
dir.create(study)
spec <- file.path(study, "dm.yaml")
writeLines('yamaa: "1.0"
type: spec
domain: DM
input: {RAW: {path: input.csv, types: {USUBJID: str, AGE: int}}}
keys: [USUBJID]
columns:
  - {name: USUBJID, type: str, derivation: RAW.USUBJID}
  - {name: AGE, type: int, derivation: RAW.AGE}
output: {path: dm.csv, columns: [USUBJID, AGE]}
', spec)

result <- yamaa_lint(study)
result$findings[, c("line", "finding", "message")]
remaining <- yamaa_lint(spec, fix = TRUE)
remaining$findings[, c("line", "finding", "message")]

unlink(study, recursive = TRUE)
}
