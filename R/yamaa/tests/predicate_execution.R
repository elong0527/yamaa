library(yamaa)
# Hand-authored typed requests and expected observations; no Python compiler runs.
truth <- read.delim(system.file("datasets", "predicate_execution.tsv", package = "yamaa"),
                    sep = "\t", quote = "", comment.char = "", colClasses = "character",
                    fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  source <- readBin(system.file("datasets", truth$input[i], package = "yamaa"), "raw", n = 8192L)
  actual <- if (truth$secondary[i] == "-") {
    execute_dataset(truth$request[i], source)
  } else {
    right <- readBin(system.file("datasets", truth$secondary[i], package = "yamaa"), "raw", n = 8192L)
    execute_dataset_sources(truth$request[i], source, list(right))
  }
  source[] <- as.raw(0)
  invisible(gc())
  stopifnot(identical(actual$outcome, truth$expected[i]))
  if (truth$snapshot[i] == "null") {
    stopifnot(is.null(actual$table))
  } else {
    stopifnot(is.raw(actual$table), identical(table_snapshot(actual$table), truth$snapshot[i]))
  }
}
# Pattern admission wins over malformed IPC and fresh valid calls still work.
request <- truth$request[1L]
invalid <- sub('"pattern":"a"', '"pattern":"["', request, fixed = TRUE)
error <- tryCatch(execute_dataset(invalid, charToRaw("invalid IPC")), error = identity)
stopifnot(inherits(error, "error"), identical(conditionMessage(error), "invalid bound dataset plan"))
invalid <- sub('"pattern":"a"', '"pattern":"a{1000001}"', request, fixed = TRUE)
error <- tryCatch(execute_dataset(invalid, charToRaw("invalid IPC")), error = identity)
stopifnot(inherits(error, "error"), identical(conditionMessage(error), "dataset plan exceeds resource limit"))
source <- readBin(system.file("datasets", truth$input[1L], package = "yamaa"), "raw", n = 8192L)
stopifnot(identical(execute_dataset(request, source)$outcome, truth$expected[1L]),
          identical(engine_info()$execution_supported, FALSE))
