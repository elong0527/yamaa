\name{int64_vector_conversions}
\alias{as.double.yamaa_int64_vector}
\alias{as.integer.yamaa_int64_vector}
\title{Exact conversion of lossless integer output}
\usage{
\method{as.double}{yamaa_int64_vector}(x, ...)
\method{as.integer}{yamaa_int64_vector}(x, ...)
}
\arguments{
\item{x}{A \code{yamaa_int64_vector} returned in public domain output.}
\item{...}{Unused.}
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
the full signed 64-bit range. Subsetting with \code{[} retains the vector class
and the same conversion checks.
}
\value{An ordinary double or integer vector, or an error if conversion would
lose a present value.}
