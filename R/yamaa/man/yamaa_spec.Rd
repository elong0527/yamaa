\name{yamaa_spec}
\alias{yamaa_spec}
\title{One specification, read only when a method runs}
\description{
Holds the path of one specification. Nothing is read until a method runs, and
each method rereads the files.
}
\usage{yamaa_spec(path)}
\arguments{
  \item{path}{Path of a specification file.}
}
\value{
An object with three methods:
\describe{
  \item{\code{$check()}}{An object whose \code{$issues} data frame lists the
    problems found without reading study data or running project code.}
  \item{\code{$resolve()}}{The specification after its layers are composed, as
    a named list.}
  \item{\code{$run()}}{A run: \code{$output} is the dataset (\code{NULL} when
    the run failed), \code{$issues} what is wrong, \code{$report} what the run
    read, counted and checked, and \code{$save()} writes the declared report
    file and the output dataset.}
}
A failure raises a condition of class \code{yamaa_error} whose \code{issues}
data frame says why.
}
\examples{
\dontrun{
run <- yamaa_spec("sdtm/dm.yaml")$run()
run$output
run$save()
}
}
