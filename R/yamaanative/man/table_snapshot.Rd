\name{table_snapshot}
\alias{table_snapshot}
\title{Copy or inspect a lossless Arrow table through Rust}
\usage{table_snapshot(request)}
\arguments{\item{request}{A raw vector containing one canonical Arrow IPC stream.}}
\value{One owned table/1 JSON string with exact typed scalar values.}
\description{Copies or inspects a bounded canonical Arrow table without narrowing integers or dropping collected temporal precision. This is table interchange, not specification execution.}
\details{Only uncompressed little-endian V5 IPC streams with the documented closed physical schema are accepted. IPC input/output and snapshot limits are 8 and 16 MiB respectively. Row, column, batch and cell budgets are fixed by the native transport. Invalid input raises an error before any external side effect. See the package README for the schema and remaining gates.}
