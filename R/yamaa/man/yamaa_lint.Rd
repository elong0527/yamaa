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
  \item{paths}{Paths of yamaa files. A directory names the yamaa files of
    every file type beneath it, outside hidden directories.}
  \item{fix}{Whether to apply the proved fixes first. A file is replaced only
    when the fixed text reads as the same values with the same comments, or
    as those values with exactly the canonical rewrite a finding names.}
}
\details{
A comment \code{# yamaa-lint: allow <finding> -- <reason>} directly above a
field or an entry suppresses the named findings within it.
}
\value{
A data frame with one row per finding that remains and the columns
\code{file} (the absolute path, with forward slashes), \code{line},
\code{column}, \code{finding}, \code{spec_path} and \code{message}. A file
that cannot be read or parsed, or a directory that cannot be listed, raises a
condition of class \code{yamaa_error}, and nothing is written.
}
\examples{
\dontrun{
findings <- yamaa_lint(c("sdtm/dm.yaml", "adam/"))
remaining <- yamaa_lint("sdtm/dm.yaml", fix = TRUE)
}
}
