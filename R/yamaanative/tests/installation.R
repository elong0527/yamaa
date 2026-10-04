library(yamaanative)
info <- engine_info()
stopifnot(
  identical(info$core_version, "0.1.0"),
  identical(info$protocol_version, "installation-probe/1"),
  identical(info$execution_supported, FALSE),
  identical(info$installation_resource, "yamaa native installation probe\n"),
  identical(info, engine_info())
)

# Exercise the same independent wire truth after package installation. Values
# stay as lossless envelopes; no ordinary R numeric vector carries an i64.
vectors <- read.delim(
  system.file("scalar_transport.tsv", package = "yamaanative"),
  sep = "\t", quote = "", comment.char = "", stringsAsFactors = FALSE,
  fileEncoding = "UTF-8", check.names = FALSE
)
for (i in seq_len(nrow(vectors))) {
  actual <- tryCatch(scalar_round_trip(vectors$request[i]), error = identity)
  expected <- vectors$expected[i]
  if (startsWith(expected, "error:")) {
    stopifnot(inherits(actual, "error"))
    stopifnot(identical(conditionMessage(actual), substring(expected, 7L)))
  } else {
    stopifnot(identical(actual, expected))
  }
}
owned <- scalar_round_trip(vectors$request[1L])
rm(vectors)
invisible(gc())
for (i in seq_len(100L)) {
  stopifnot(identical(scalar_round_trip(owned), owned))
}
for (invalid in list(NA_character_, character(), c("a", "b"), 1, NULL)) {
  stopifnot(inherits(tryCatch(scalar_round_trip(invalid), error = identity), "error"))
}
too_large <- paste(rep("x", 1048577L), collapse = "")
failure <- tryCatch(scalar_round_trip(too_large), error = identity)
stopifnot(
  inherits(failure, "error"),
  identical(conditionMessage(failure), "scalar transport request exceeds byte limit"),
  identical(scalar_round_trip(owned), owned)
)
