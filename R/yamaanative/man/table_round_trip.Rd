\name{table_round_trip}
\alias{table_round_trip}
\title{Copy or inspect a lossless Arrow table through Rust}
\usage{table_round_trip(request)}
\arguments{\item{request}{A raw vector containing one canonical Arrow IPC stream.}}
\value{An owned sanitized raw IPC vector.}
\description{Copies or inspects a bounded canonical Arrow table without narrowing integers or dropping collected temporal precision. This is table interchange, not specification execution.}
\details{Only uncompressed little-endian V5 IPC streams with the documented closed physical schema are accepted. IPC input/output and snapshot limits are 8 and 16 MiB respectively. Row, column, batch and cell budgets are fixed by the native transport. Invalid input raises an error before any external side effect. See the package README for the schema and remaining gates.}
