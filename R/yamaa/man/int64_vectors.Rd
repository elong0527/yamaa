\name{int64_vector_conversions}
\alias{as.double.yamaa_int64_vector}
\alias{as.integer.yamaa_int64_vector}
\alias{[[.yamaa_int64_vector}
\alias{Ops.yamaa_int64_vector}
\title{Extract, compare and convert lossless integer output}
\usage{
\method{as.double}{yamaa_int64_vector}(x, ...)
\method{as.integer}{yamaa_int64_vector}(x, ...)
\method{[[}{yamaa_int64_vector}(x, ...)
\method{Ops}{yamaa_int64_vector}(e1, e2)
}
\arguments{
\item{x}{A \code{yamaa_int64_vector} returned in public domain output.}
\item{...}{Unused for conversion; ordinary subscript arguments for extraction.}
\item{e1, e2}{Integer output vectors, lossless integer scalars or ordinary
unclassed numeric vectors to compare.}
}
\description{
Convert canonical signed 64-bit output values through the shared Rust scalar
checks. Double conversion, including \code{as.numeric}, succeeds only when every
present value is exactly representable. Integer conversion succeeds only within
the non-missing R integer range, from -2147483647 through 2147483647.
}
\details{
Names and missing values are preserved. INT64_MIN is a present value and can be
converted exactly to double. The value -2147483648 cannot be converted to an R
integer because R reserves that bit pattern for missingness. An inexact or
out-of-range present value raises an error for the conversion without returning
a partially converted vector. The original output remains unchanged.

Use \code{as.character} for an exact decimal representation of every value in
the full signed 64-bit range. Subsetting with \code{[} and extraction with
\code{[[} retain the vector class and the same conversion checks.

The six comparison operators \code{==}, \code{!=}, \code{<}, \code{<=},
\code{>} and \code{>=} compare admitted scalar values through Rust, including
exact integer/double comparisons beyond 2^53. Missing operands produce missing
logical values. Ordinary vector recycling and names are preserved; fractional
recycling warns. The method rejects incompatible types and malformed integer
storage. Other vector operators currently raise an explicit unsupported-operator
error. Comparison does not convert integer output to character or double.
Interactions with objects defining their own \code{Ops} methods, such as
\code{Date}, remain unqualified: R can bypass both methods after an incompatible
methods warning. Use ordinary unclassed numeric operands or lossless integer
scalars for the comparisons covered here.
}
\value{Conversion returns an ordinary double or integer vector, or an error
if it would lose a present value. Extraction returns one integer output value
with its class. Comparison returns an ordinary logical vector.}
