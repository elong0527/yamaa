\name{yamaa_domain}
\alias{yamaa_domain}
\alias{yamaa_check}
\title{Build or Check a Domain with the Shared Rust Engine}
\usage{
yamaa_domain(specification, environment = NULL, resolver = NULL)
yamaa_check(specification, environment = NULL, resolver = NULL)
}
\arguments{
\item{specification}{One domain-specification path.}
\item{environment}{Optional study environment path for functions and codelists.}
\item{resolver}{Optional host resource resolver.}
}
\value{A build result containing output, issues, diagnostics, optional logs and
a save method, or a check result containing issues and diagnostics. \code{issues}
is a data frame with character \code{phase}, \code{condition}, \code{requirement}
(NA when absent), a list \code{spec_paths} column and JSON-text \code{context};
\code{diagnostics} is a list of records built natively from the same findings,
each with \code{phase}, \code{condition}, \code{requirement} (NULL when absent),
\code{spec_paths}, a structured \code{context} (objects as named lists, arrays as
unnamed lists, integer tokens of magnitude at most 2^53 as doubles, wider integer
tokens as character decimal tokens, other numbers as doubles),
\code{source_span} (\code{list(start, end)} byte offsets or
NULL when unrecorded) and \code{operand_route} (character steps from the expression
root, \code{character(0)} for a failure at the root, or NULL when unrecorded).
Integer columns are doubles and text columns character, with \code{NA} for
missing. An integer beyond 2^53 in magnitude or text containing NUL has no exact
R value, so reading it raises an error instead of changing it.}
\details{With nanoarrow 0.8.0 or newer,
\code{output_arrow}, \code{verification_log_arrow} and \code{warning_log_arrow}
are independent Arrow C Streams sharing retained immutable buffers. Nanoarrow
owns each stream and releases it explicitly or on garbage collection; arrays
already imported from a stream outlive both the stream and the build result.
The ordinary data-frame views are explicit copying conversions.}
\description{Runs native preparation and static checks. A build additionally captures
study inputs and derives output. Save publishes retained bytes to the declared path.
Failed-build saves signal \code{yamaa_domain_error}; operational save failures return
FALSE and remain in issues. Relative resources resolve from the YAML file that
declares them, including inherited layers.}
