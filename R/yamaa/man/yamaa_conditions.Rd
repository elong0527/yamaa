\name{yamaa_conditions}
\alias{yamaa_conditions}
\title{The conditions of this release and their history}
\description{
Returns the condition catalogue: every condition the engine may report in an
issue, and every condition a release retired, with what a program needs to
match it. The catalogue is part of the installed engine, so nothing is read
from disk, and the same table comes from every host. The file
\code{yaml/conditions.yaml} of the source repository at a release's tag holds
the same catalogue.
}
\usage{
yamaa_conditions()
}
\value{
A data frame with one row per condition, in name order, and these columns:
\describe{
  \item{\code{condition}}{The condition, as an issue's \code{condition}
    column names it.}
  \item{\code{severity}}{The severity of its issues: \code{"error"} for every
    condition.}
  \item{\code{phases}}{A list column: the phases that may report the
    condition, as a character vector; empty for a retired condition.}
  \item{\code{rules}}{A list column: the rule cards that its issues cite, such
    as \code{"VALUE-3"}, in card order; empty for a condition whose issues
    cite no rule and for a retired condition.}
  \item{\code{message}}{The template its issues' messages are written from;
    \code{NA} for a retired condition. Messages may change in any release,
    so match on \code{condition} and \code{rule}, never on the message.}
  \item{\code{renamed_from}}{The name the condition had before a release
    renamed it, or \code{NA}.}
  \item{\code{retired_in}}{The release that retired the condition, such as
    \code{"1.0.0"}, or \code{NA} for a condition the engine may report.}
}
}
\examples{
catalogue <- yamaa_conditions()
nrow(catalogue)
catalogue[!is.na(catalogue$renamed_from), c("condition", "renamed_from")]
catalogue$condition[!is.na(catalogue$retired_in)]
}
