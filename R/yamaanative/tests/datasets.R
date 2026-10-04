library(yamaanative)
truth <- read.delim(system.file("datasets", "expected.tsv", package = "yamaanative"),
                    sep = "\t", quote = "", comment.char = "", colClasses = "character",
                    fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  source <- readBin(system.file("datasets", truth$input[i], package = "yamaanative"),
                    "raw", n = 8L * 1024L * 1024L)
  names <- if (truth$secondary[i] != "-") strsplit(truth$secondary[i], ";", fixed = TRUE)[[1L]] else character()
  secondary <- lapply(names, function(name) readBin(system.file("datasets", name, package = "yamaanative"), "raw", n = 8L * 1024L * 1024L))
  actual <- if (length(secondary)) execute_dataset_sources(truth$request[i], source, secondary) else execute_dataset(truth$request[i], source)
  rm(secondary)
  source[] <- as.raw(0)
  rm(source)
  invisible(gc())
  stopifnot(identical(actual$outcome, truth$expected[i]))
  if (nzchar(truth$snapshot[i]) && truth$snapshot[i] != "null") {
    stopifnot(is.raw(actual$table),
              identical(table_snapshot(actual$table), truth$snapshot[i]),
              identical(table_round_trip(actual$table), actual$table))
  } else {
    stopifnot(is.null(actual$table))
  }
}
request <- paste(readLines(system.file("datasets", "adlb-plan.json", package = "yamaanative")), collapse = "\n")
source <- readBin(system.file("datasets", "adlb.arrow", package = "yamaanative"), "raw", n = 8192L)
for (invalid in list(NA_character_, character(), c("a", "b"), 1, structure(request, class = "wrapped"))) {
  stopifnot(inherits(tryCatch(execute_dataset(invalid, source), error = identity), "error"))
}
stopifnot(inherits(tryCatch(execute_dataset(request, "bytes"), error = identity), "error"))
invalid <- rawToChar(as.raw(c(0xc0, 0xaf)))
Encoding(invalid) <- "UTF-8"
stopifnot(inherits(tryCatch(execute_dataset(invalid, source), error = identity), "error"))
result <- .Call(yamaanative:::wrap__execute_dataset, as.raw(c(0xc0, 0xaf)), source)
stopifnot(identical(result$error, "invalid UTF-8 JSON request"), is.null(result$value))
invalid_plan <- sub('"source": 0', '"source": 999', request, fixed = TRUE)
error <- tryCatch(execute_dataset(invalid_plan, charToRaw("invalid IPC")), error = identity)
stopifnot(inherits(error, "error"), identical(conditionMessage(error), "invalid bound dataset plan"))
actual <- execute_dataset(request, source)
stopifnot(is.raw(actual$table), identical(actual$outcome, truth$expected[1]),
          identical(engine_info()$execution_supported, FALSE))

# Feature discovery is shared metadata and performs no source loading.
stopifnot(identical(dataset_capabilities(),
  '{"protocol":"dataset/1","features":["row_filter","predicate_checks","key_grain","window_numbering","window_filter","window_values","window_baseline","root_filter","source_filter","source_selection","multi_source","named_intermediate","numeric_compute"]}'))

# Secondary snapshots are an explicit bounded list; legacy calls cannot omit them.
lookup <- which(truth$case == "lookup_values")
request <- truth$request[lookup]
source <- readBin(system.file("datasets", truth$input[lookup], package = "yamaanative"), "raw", n = 8192L)
right <- readBin(system.file("datasets", truth$secondary[lookup], package = "yamaanative"), "raw", n = 8192L)
for (invalid in list("bytes", list("bytes"), rep(list(raw()), 8L))) {
  stopifnot(inherits(tryCatch(execute_dataset_sources(request, source, invalid), error = identity), "error"))
}
stopifnot(inherits(tryCatch(execute_dataset(request, source), error = identity), "error"))
actual <- execute_dataset_sources(request, source, list(right))
stopifnot(identical(actual$outcome, truth$expected[lookup]), identical(table_snapshot(actual$table), truth$snapshot[lookup]))
