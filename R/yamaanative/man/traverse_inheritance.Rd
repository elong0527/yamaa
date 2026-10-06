\name{traverse_inheritance}
\alias{traverse_inheritance}
\title{Traverse inherited layers through shared Rust with explicit source authority}
\usage{
traverse_inheritance(schema_request, request, callback)
}
\arguments{
\item{schema_request}{One schema/1 compile request as JSON text.}
\item{request}{One inheritance/1 entry request as JSON text.}
\item{callback}{Explicit function taking request text and maximum reply bytes.}
}
\value{Owned inheritance/1 outcome JSON with normalized contributions or failure.}
\description{
Normalizes each layer, checks versions and visits parents in deterministic
postorder. The callback implements canonicalize and read operations, receiving
request JSON and the remaining cumulative reply-byte allowance and returning
reply JSON. No filesystem reader is supplied implicitly. Original callback
errors and interruptions are raised after Rust returns. This does not enable
workflow execution.
}
