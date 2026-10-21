\name{yamaa_schema}
\alias{yamaa_schema}
\title{The JSON Schema of a yamaa file type}
\description{
Returns the JSON Schema that a file of one type is checked against, so that an
editor can validate and complete the file as it is typed. The schema is part
of the installed engine, so nothing is read from disk, and the same text comes
from every host.
}
\usage{
yamaa_schema(file_type)
}
\arguments{
  \item{file_type}{The file type, as a file writes it in its \code{type}
    header: \code{"spec"}, \code{"project"}, \code{"codelists"} or
    \code{"function"}.}
}
\details{
A specification file may be one layer of an inheritance chain, so the
\code{"spec"} schema checks what one layer may write, and its
\code{$defs/root_class} checks a complete specification. Every field carries
its kind as \code{x-yamaa-kind} and its schema-notation type as
\code{x-yamaa-type}.
}
\value{
One character string: the JSON Schema as canonical JSON text, on one line with
its members in key order. Any other file type raises a condition of class
\code{yamaa_error} whose issue names the condition
\code{unexpected_file_type}.
}
\examples{
schema <- yamaa_schema("spec")
nchar(schema)
substr(schema, 1, 40)
}
