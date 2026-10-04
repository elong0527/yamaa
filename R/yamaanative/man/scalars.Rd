\name{int64}
\alias{int64}
\alias{utf8_scalar}
\alias{utf8_bytes}
\title{Lossless integer and UTF-8 scalars}
\usage{
int64(x)
utf8_scalar(x)
utf8_bytes(x)
}
\arguments{
\item{x}{For \code{int64}, a canonical decimal character scalar or non-missing R
integer. For \code{utf8_scalar}, an unclassed raw UTF-8 vector or character scalar.
For \code{utf8_bytes}, a UTF-8 or ordinary character scalar.}
}
\value{\code{int64} returns a validated \code{yamaa_int64} scalar;
\code{utf8_scalar} returns a validated \code{yamaa_utf8} scalar;
\code{utf8_bytes} returns owned raw bytes.}
\description{Represent existing logical integer and string values without narrowing
full-range i64, colliding with NA, or losing embedded NUL bytes. Payloads are
validated by Rust before use. Missing is separate from these present scalars.}
\details{Integer storage is one canonical decimal character value; UTF-8 storage
is a raw scalar whose byte count is not a count of logical values. Only the exact
class and validated storage are accepted; arbitrary wrappers are not scalars.
Raw input must be valid UTF-8. Byte-marked R character strings are rejected;
declared Latin-1 characters are decoded explicitly. Other R character bytes are
validated as UTF-8 without locale fallback or repair.

Arithmetic \code{+}, \code{-}, \code{*}, \code{/} and comparisons use
shared Rust scalar semantics. Unary plus/minus are supported. There is no vector
recycling, Boolean-to-number conversion or implicit text conversion. Integer
operations check overflow. Mixed float operands promote to binary64; use \code{1L}
or \code{int64("1")} for an integer operand. Nonfinite values normalize to missing.

\code{as.character} preserves integer decimal text and NUL-free UTF-8 text.
UTF-8 text containing NUL requires \code{utf8_bytes}; conversion to R character
fails explicitly. \code{as.double} and \code{as.integer} accept an integer only
when the destination can represent it exactly as a non-missing value. Scalar
operators return these lossless classes for int/str, ordinary R double/logical
for float/bool, or logical NA for missing.}
\examples{
x <- int64("9007199254740993")
as.character(x + 1L)
x > 9007199254740992
s <- utf8_scalar(as.raw(c(97L, 0L, 98L)))
utf8_bytes(s)
s == utf8_scalar(as.raw(c(97L, 0L, 98L)))
}
