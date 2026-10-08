\name{yamaa_domain}
\alias{yamaa_domain}
\alias{yamaa_check}
\alias{as.character.yamaa_int64_vector}
\alias{[.yamaa_int64_vector}
\alias{is.na.yamaa_utf8_vector}
\alias{[.yamaa_utf8_vector}
\title{Build or Check a Domain with the Shared Rust Engine}
\usage{
yamaa_domain(specification, environment = NULL)
yamaa_check(specification, environment = NULL)
\method{as.character}{yamaa_int64_vector}(x, ...)
}
\arguments{
\item{specification}{One domain-specification path.}
\item{environment}{Optional environment path. Unsupported environments are issues.}
\item{x}{A lossless signed 64-bit output vector.}
\item{...}{Unused arguments.}
}
\value{A build result containing output, issues, optional logs and a save method,
or a check result containing issues. Integer columns use canonical decimal text
with the \code{yamaa_int64_vector} class and preserve the full signed 64-bit range.
Text containing NUL uses raw values in a \code{yamaa_utf8_vector}; NULL is missing.}
\description{Runs native preparation and static checks. A build additionally captures
study inputs and derives output. Save publishes retained bytes to the declared path.
Failed-build saves signal \code{yamaa_domain_error}; operational save failures return
FALSE and remain in issues. The current approved-root path policy remains in effect.}
