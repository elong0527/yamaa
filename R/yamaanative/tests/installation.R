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
check_vectors <- function(fixture, invoke) {
  vectors <- read.delim(
    system.file(fixture, package = "yamaanative"),
    sep = "\t", quote = "", comment.char = "", stringsAsFactors = FALSE,
    fileEncoding = "UTF-8", check.names = FALSE
  )
  for (i in seq_len(nrow(vectors))) {
    actual <- tryCatch(invoke(vectors$request[i]), error = identity)
    expected <- vectors$expected[i]
    if (startsWith(expected, "error:")) {
      stopifnot(inherits(actual, "error"))
      stopifnot(identical(conditionMessage(actual), substring(expected, 7L)))
    } else {
      stopifnot(identical(actual, expected))
    }
  }
  vectors
}
vectors <- check_vectors("scalar_transport.tsv", scalar_round_trip)
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


# The numeric API invokes shared core compilation and engine lifecycle handling.
numeric_vectors <- check_vectors("numeric_transport.tsv", evaluate_numeric)
owned_numeric <- evaluate_numeric(numeric_vectors$request[1L])
request_numeric <- '{"protocol":"numeric/1","expression":"1.5","column_path":"columns.A","target":"int","bindings":[],"unconvertible":{"value":{"int":"7"}}}'
handled_numeric <- evaluate_numeric(request_numeric)
for (i in seq_len(100L)) {
  stopifnot(identical(evaluate_numeric(request_numeric), handled_numeric))
}
stopifnot(grepl('"count":"1"', handled_numeric, fixed = TRUE))
for (invalid in list(NA_character_, character(), c("a", "b"), 1, NULL, too_large)) {
  stopifnot(inherits(tryCatch(evaluate_numeric(invalid), error = identity), "error"))
}
limit_request <- paste0('{"protocol":"numeric/1","expression":"',
                        paste(rep("A", 65537L), collapse = ""),
                        '","column_path":"columns.A","target":"int","bindings":[]}')
limit_result <- evaluate_numeric(limit_request)
stopifnot(
  grepl('"status":"limit"', limit_result, fixed = TRUE),
  grepl('"resource":"bytes","limit":"65536"', limit_result, fixed = TRUE),
  grepl('"handler_counts":[],"resolutions":[]', limit_result, fixed = TRUE),
  identical(evaluate_numeric(numeric_vectors$request[1L]), owned_numeric)
)
rm(numeric_vectors, request_numeric, limit_request)
invisible(gc())
stopifnot(grepl('"status":"value"', owned_numeric, fixed = TRUE))

# Raw IPC keeps full i64 and temporal precision without an R Arrow dependency.
table_truth <- read.delim(system.file("tables", "expected.tsv", package = "yamaanative"),
                          sep = "\t", quote = "", comment.char = "",
                          colClasses = "character", fileEncoding = "UTF-8")
for (case in c("mixed", "empty", "schema_only", "zero_columns")) {
  path <- system.file("tables", paste0(case, ".arrow"), package = "yamaanative")
  request <- readBin(path, "raw", n = file.info(path)$size)
  truth <- table_truth$expected[table_truth$id == case]
  stopifnot(identical(table_snapshot(request), truth))
  result <- table_round_trip(request)
  rm(request)
  for (i in seq_len(5)) {
    stopifnot(is.raw(result), identical(table_snapshot(result), truth))
    result <- table_round_trip(result)
  }
}
for (invalid in list(NULL, "text", 1L, list())) {
  stopifnot(inherits(try(table_round_trip(invalid), silent = TRUE), "try-error"))
}
for (invalid in list(raw(), charToRaw("ARROW1"), as.raw(rep(255L, 8L)))) {
  stopifnot(inherits(try(table_snapshot(invalid), silent = TRUE), "try-error"))
}
stopifnot(inherits(try(table_round_trip(raw(8L * 1024L * 1024L + 1L)), silent = TRUE), "try-error"))

# An R UTF-8 encoding mark is not evidence that the held bytes are valid.
# Exercise both facade and direct raw native entrypoints after installation.
json_apis <- list(
  list(call = scalar_round_trip, symbol = "wrap__scalar_round_trip",
       prefix = '{"protocol":"scalar/1","value":{"str":"', suffix = '"}}',
       limit = "scalar transport request exceeds byte limit"),
  list(call = evaluate_numeric, symbol = "wrap__evaluate_numeric",
       prefix = '{"protocol":"numeric/1","expression":"1","column_path":"',
       suffix = '","target":"int","bindings":[]}',
       limit = "numeric transport request exceeds resource limit")
)
bad_utf8 <- list(c(255L), c(192L, 128L), c(237L, 160L, 128L),
                 c(244L, 144L, 128L, 128L), c(226L, 130L))
for (api in json_apis) {
  symbol <- get(api$symbol, envir = asNamespace("yamaanative"))
  valid <- paste0(api$prefix, "caf\u00e9", api$suffix)
  expected <- api$call(valid)
  latin1 <- rawToChar(c(charToRaw(api$prefix), charToRaw("caf"),
                        as.raw(233L), charToRaw(api$suffix)))
  Encoding(latin1) <- "latin1"
  stopifnot(identical(api$call(latin1), expected))
  stopifnot(identical(.Call(symbol, charToRaw(enc2utf8(valid)))$value, expected))
  for (invalid in bad_utf8) {
    bytes <- c(charToRaw(api$prefix), as.raw(invalid), charToRaw(api$suffix))
    direct <- .Call(symbol, bytes)
    stopifnot(is.null(direct$value), identical(direct$error, "invalid UTF-8 JSON request"))
    for (encoding in c("unknown", "UTF-8")) {
      text <- rawToChar(bytes)
      Encoding(text) <- encoding
      failure <- tryCatch(api$call(text), error = identity)
      stopifnot(inherits(failure, "error"),
                identical(conditionMessage(failure), "invalid UTF-8 JSON request"))
    }
  }
  marked <- valid
  Encoding(marked) <- "bytes"
  failure <- tryCatch(api$call(marked), error = identity)
  stopifnot(inherits(failure, "error"), identical(conditionMessage(failure),
            "byte-marked R text has no declared Unicode encoding"))
  for (invalid in list(structure(valid, class = "unknown"), setNames(valid, "x"))) {
    stopifnot(inherits(tryCatch(api$call(invalid), error = identity), "error"))
  }
  # Check size before scanning invalid bytes, without exposing them in errors.
  over_limit <- as.raw(rep(255L, 1048577L))
  stopifnot(identical(.Call(symbol, over_limit)$error, api$limit))
  invisible(gc())
  stopifnot(identical(api$call(valid), expected))
}
