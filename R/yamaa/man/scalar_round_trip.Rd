\name{scalar_round_trip}
\alias{scalar_round_trip}
\title{Round-trip one lossless scalar envelope through Rust}
\usage{scalar_round_trip(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{One owned JSON character string. Invalid requests raise an error.}
\description{Decodes a versioned scalar envelope to a core value and returns
owned JSON text. Full i64 integers use decimal strings, floats use binary64 bit
strings, and temporal values retain collected precision. The probe evaluates no
specifications and does not convert results to ordinary R numeric vectors.}
\details{The scalar/1 envelope contains protocol and value fields. For example,
\code{\{"protocol":"scalar/1","value":\{"int":"9007199254740993"\}\}}.
The request limit is 1,048,576 UTF-8 bytes. Declared Latin-1 is decoded explicitly;
byte-marked text and malformed UTF-8 are rejected without locale fallback or
replacement. Native entrypoints validate raw bytes before constructing Rust
strings. Nonfinite floats normalize to missing.
See the package README for all variants and remaining qualification gates.}
\examples{
scalar_round_trip('{"protocol":"scalar/1","value":{"int":"9007199254740993"}}')
}
