# Installed nanoarrow C Stream handoff; exact values are authored independently.
library(yamaa)
stopifnot(requireNamespace("nanoarrow", quietly = TRUE),
  utils::packageVersion("nanoarrow") >= "0.8.0")

run_arrow_bridge <- function() {
  root <- tempfile("yamaa-arrow-bridge-"); dir.create(root)
  on.exit(unlink(root, recursive = TRUE))
  spec <- paste0(
    "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\n",
    "input:\n  SRC:\n    path: input.csv\n",
    "    types: {ID: int, S: str, I: int, F: float, D: date, T: datetime}\n",
    "columns:\n",
    "  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n",
    "  - {name: S, type: str, label: Text, derivation: SRC.S}\n",
    "  - {name: I, type: int, label: Integer, derivation: SRC.I}\n",
    "  - {name: F, type: float, label: Float, derivation: SRC.F}\n",
    "  - {name: D, type: date, label: Date, derivation: SRC.D}\n",
    "  - {name: T, type: datetime, label: Datetime, derivation: SRC.T}\n",
    "output: {path: output.csv, columns: [ID, S, I, F, D, T]}\n")
  input <- c(charToRaw("ID,S,I,F,D,T\n1,a"), as.raw(0), charToRaw(paste0(
    "\U0001f980,-9223372036854775808,-0.0,0001-01-01,0001-01-01T00:00:00\n",
    "2,,,,,\n3,\u03b2,9223372036854775807,1.25,9999-12-31,9999-12-31T23:59:59\n")))
  writeBin(charToRaw(spec), file.path(root, "spec.yaml"))
  writeBin(input, file.path(root, "input.csv"))
  result <- yamaa_domain(file.path(root, "spec.yaml"))
  stopifnot(nrow(result$issues) == 0L)
  frame <- result$output
  stopifnot(identical(as.character(frame$I), c("-9223372036854775808", NA_character_, "9223372036854775807")),
    identical(unclass(frame$S)[[1L]], c(charToRaw("a"), as.raw(0), charToRaw("\U0001f980"))),
    identical(is.na(frame$S), c(FALSE, TRUE, FALSE)),
    inherits(tryCatch(as.character(frame$S), error = identity), "error"),
    inherits(tryCatch(as.double(frame$I), error = identity), "error"),
    identical(unclass(frame$D), c(-719162, NA_real_, 2932896)),
    identical(as.numeric(frame$T), c(-62135596800, NA_real_, 253402300799)))
  # The run report crosses the same bridge with its two int64 counts.
  report <- result$report_arrow
  stopifnot(identical(vapply(report$get_schema()$children, function(child) child$format,
    character(1L), USE.NAMES = FALSE), c(rep("u", 10L), "l", "l", "u", "u")))
  rows <- report$get_next()
  report$release()
  stopifnot(identical(rows$length, 9L),
    identical(result$report$KIND, c("run", "input", "rows", rep("column", 6L))))
  source <- result$output_arrow
  schema <- source$get_schema()
  stopifnot(identical(vapply(schema$children, function(child) child$format, character(1L), USE.NAMES = FALSE),
    c("l", "u", "l", "g", "tdD", "tss:")))
  first <- source$get_next()
  source$release()
  stopifnot(!nanoarrow::nanoarrow_pointer_is_valid(source), identical(first$length, 3L))
  buffers <- function(array) lapply(array$children, function(child) {
    vapply(child$buffers, function(buffer) nanoarrow::nanoarrow_pointer_addr_chr(buffer$data), character(1L))
  })
  held_addresses <- buffers(first)
  for (i in seq_len(30L)) {
    # Each native export owns a fresh stream cursor over the same buffers.
    stream <- result$output_arrow
    imported <- stream$get_next()
    stopifnot(identical(buffers(imported), held_addresses))
    stream$release()
    rm(stream); gc()
  }
  # A failed host conversion releases its stream; the retained build is usable.
  failed <- result$output_arrow
  convert <- function(stream) {
    on.exit(stream$release())
    nanoarrow::convert_array_stream(stream, to = integer())
  }
  error <- tryCatch(convert(failed), error = identity)
  stopifnot(inherits(error, "error"), !nanoarrow::nanoarrow_pointer_is_valid(failed),
    identical(as.character(result$output$I), as.character(frame$I)))
  # Native Arrow consumers move stream ownership and retain imported arrays.
  stream <- result$output_arrow
  moved <- nanoarrow::nanoarrow_allocate_array_stream()
  nanoarrow::nanoarrow_pointer_export(stream, moved)
  stopifnot(!nanoarrow::nanoarrow_pointer_is_valid(stream))
  imported <- moved$get_next()
  moved$release()
  rm(result, source, stream, moved); gc()
  stopifnot(identical(buffers(imported), held_addresses),
    identical(as.raw(imported$children$S$buffers[[3L]]),
      c(charToRaw("a"), as.raw(0), charToRaw("\U0001f980\u03b2"))))
  # Full-width i64 values stay binary exact, including the minimum value.
  integer_bytes <- as.raw(imported$children$I$buffers[[2L]])
  stopifnot(identical(integer_bytes[1:8], as.raw(c(rep(0L, 7L), 128L))),
    identical(integer_bytes[17:24], as.raw(c(rep(255L, 7L), 127L))))
  empty <- nanoarrow::nanoarrow_array_modify(first, list(length = 0))
  for (i in seq_len(30L)) {
    chunked <- nanoarrow::basic_array_stream(list(empty, first, empty, imported), schema = schema)
    destination <- nanoarrow::nanoarrow_allocate_array_stream()
    nanoarrow::nanoarrow_pointer_export(chunked, destination)
    chunks <- nanoarrow::collect_array_stream(destination)
    destination$release()
    stopifnot(identical(vapply(chunks, function(chunk) chunk$length, numeric(1L)), c(0, 3, 0, 3)),
      identical(buffers(chunks[[2L]]), held_addresses))
  }
  writeBin(charToRaw("ID,S,I,F,D,T\n"), file.path(root, "input.csv"))
  result <- yamaa_domain(file.path(root, "spec.yaml"))
  stream <- result$output_arrow
  stopifnot(identical(stream$get_next()$length, 0L), is.null(stream$get_next()),
    identical(nrow(result$output), 0L))
  stream$release()
}
run_arrow_bridge()
