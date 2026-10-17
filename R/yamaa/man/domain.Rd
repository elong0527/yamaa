\name{yamaa_domain}
\alias{yamaa_domain}
\alias{yamaa_check}
\alias{as.character.yamaa_int64_vector}
\alias{as.character.yamaa_utf8_vector}
\alias{[.yamaa_int64_vector}
\alias{is.na.yamaa_utf8_vector}
\alias{[.yamaa_utf8_vector}
\title{Build or Check a Domain with the Shared Rust Engine}
\usage{
yamaa_domain(specification, environment = NULL, resolver = NULL)
yamaa_check(specification, environment = NULL, resolver = NULL)
\method{as.character}{yamaa_int64_vector}(x, ...)
\method{as.character}{yamaa_utf8_vector}(x, ...)
}
\arguments{
\item{specification}{One domain-specification path.}
\item{environment}{Optional study environment path for functions and codelists.}
\item{resolver}{Optional host resource resolver.}
\item{x}{A lossless signed 64-bit integer or UTF-8 output vector.}
\item{...}{Unused arguments.}
}
\value{A build result containing output, issues, diagnostics, the run report and
a save method, or a check result containing issues and diagnostics. \code{report}
is the run report data frame every executed build returns, success or failure,
with one row for the run, each input, each row template, each counted handler,
each output column of a successful run and each evaluated check; its
\code{COUNT} and \code{TOTAL} columns use \code{yamaa_int64_vector} and the other
columns are character. It is NULL only when the specification is rejected before
admission. Save writes it to \code{output.report} when the specification declares
that file, a failed build included. \code{issues}
is a data frame with character \code{phase}, \code{condition}, \code{requirement}
(NA when absent), a list \code{spec_paths} column and JSON-text \code{context};
\code{diagnostics} is a list of records built natively from the same findings,
each with \code{phase}, \code{condition}, \code{requirement} (NULL when absent),
\code{spec_paths}, a structured \code{context} (objects as named lists, arrays as
unnamed lists, integer tokens of magnitude at most 2^53 as doubles, wider integer
tokens as character decimal tokens, other numbers as doubles, text containing NUL
as \code{yamaa_utf8}), \code{source_span} (\code{list(start, end)} byte offsets or
NULL when unrecorded) and \code{operand_route} (character steps from the expression
root, \code{character(0)} for a failure at the root, or NULL when unrecorded).
Integer columns use canonical decimal text
with the \code{yamaa_int64_vector} class and preserve the full signed 64-bit range.
Text containing NUL uses raw values in a \code{yamaa_utf8_vector}; NULL is missing.}
\details{\code{as.character} converts a UTF-8 vector or subset to exact Unicode
text, preserving names and missingness. If any present value contains NUL, the
conversion raises an error because R character cannot preserve that value.
Use \code{unclass} to access the held raw bytes. Conversion leaves the original
output and \code{save()} publication unchanged. With nanoarrow 0.8.0 or newer,
\code{output_arrow} and \code{report_arrow} are independent Arrow C Streams
sharing retained immutable buffers. Nanoarrow owns each stream and releases it
explicitly or on garbage collection; arrays already imported from a stream
outlive both the stream and the build result. The ordinary data-frame views are
explicit copying conversions.}
\description{Runs native preparation and static checks. A build additionally captures
study inputs and derives output. Save publishes retained bytes to the declared paths,
the declared report file before the output. Failed-build saves write only the declared
report file and signal \code{yamaa_domain_error}; operational save failures return
FALSE and remain in issues. Relative resources resolve from the YAML file that
declares them, including inherited layers.}
