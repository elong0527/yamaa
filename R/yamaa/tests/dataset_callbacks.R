library(yamaa)
truth <- read.delim(system.file("datasets", "callbacks.tsv", package = "yamaa"),
                    sep = "\t", quote = "", comment.char = "", colClasses = "character",
                    fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  source <- readBin(system.file("datasets", truth$input[i], package = "yamaa"),
                    "raw", n = 8L * 1024L * 1024L)
  for (attempt in 1:2) {
    trace <- character()
    make_callback <- function(slot) {
      force(slot)
      function(...) {
        args <- list(...)
        stopifnot(identical(names(args), if (length(args) == 3L) c("lhs", "rhs", "scale") else "value"))
        trace <<- c(trace, paste0(slot, ":", paste(vapply(args, as.character, ""), collapse = ",")))
        if (truth$mode[i] == "raise_second" && length(trace) == 2L) {
          stop(structure(list(message = "after first effect"), class = c("FixtureError", "error", "condition")))
        }
        if (truth$mode[i] == "boolean") return(TRUE)
        if (truth$mode[i] == "bad_text") return("bad")
        if (length(args) == 1L) return(args[[1L]])
        as.integer(sum(vapply(args, as.integer, 0L)))
      }
    }
    callbacks <- if (truth$case[i] == "dependent_column") list(make_callback(0L), make_callback(1L)) else list(make_callback(0L))
    actual <- execute_dataset_functions(truth$request[i], source, list(), callbacks)
    stopifnot(identical(actual$outcome, truth$expected[i]), identical(paste(trace, collapse = ";"), truth$trace[i]))
    if (truth$snapshot[i] == "null") stopifnot(is.null(actual$table)) else {
      stopifnot(identical(table_snapshot(actual$table), truth$snapshot[i]))
    }
  }
}
request <- truth$request[1L]
source <- readBin(system.file("datasets", truth$input[1L], package = "yamaa"), "raw", n = 8192L)
for (callbacks in list(list(), list(NULL), rep(list(function(...) 1L), 65L))) {
  stopifnot(inherits(tryCatch(execute_dataset_functions(request, charToRaw("bad IPC"), list(), callbacks), error = identity), "error"))
}
error <- tryCatch(execute_dataset(request, charToRaw("bad IPC")), error = identity)
stopifnot(identical(conditionMessage(error), "dataset callback bindings do not match the admitted signatures"))
interrupted <- structure(list(message = "stop this dataset"), class = c("dataset_stop", "interrupt", "condition"))
calls <- 0L
stop_callback <- function(...) { calls <<- calls + 1L; stop(interrupted) }
actual <- tryCatch(execute_dataset_functions(request, source, list(), list(stop_callback)), interrupt = identity)
stopifnot(identical(actual, interrupted), identical(calls, 1L))
actual <- execute_dataset_functions(request, source, list(), list(function(...) as.integer(sum(vapply(list(...), as.integer, 0L)))))
stopifnot(identical(actual$outcome, truth$expected[1L]), identical(table_snapshot(actual$table), truth$snapshot[1L]))

# Copy-on-modify of the caller's callback list cannot replace a later bound slot.
i <- which(truth$case == "dependent_column")
seen <- integer()
callbacks <- list(
  function(...) { callbacks <<- list(); as.integer(sum(vapply(list(...), as.integer, 0L))) },
  function(value) { seen <<- c(seen, as.integer(value)); value }
)
actual <- execute_dataset_functions(truth$request[i], source, list(), callbacks)
stopifnot(identical(seen, c(35L, 95L)), identical(table_snapshot(actual$table), truth$snapshot[i]))

# Host-name validation precedes even malformed IPC and never calls project code.
invalid <- sub('"host_name":"lhs"', '"host_name":"if"', request, fixed = TRUE)
error <- tryCatch(execute_dataset_functions(invalid, charToRaw("bad IPC"), list(), list(function(...) stop("unexpected call"))), error = identity)
stopifnot(identical(conditionMessage(error), "invalid function host argument name"))

# Collected temporal precision drops at callback encoding; binary64 bits survive.
for (kind in c("float", "date", "datetime")) {
  literal <- switch(kind,
    float = '{"float":"8000000000000000"}',
    date = '{"date":{"text":"2024-05-01","precision":"month"}}',
    datetime = '{"datetime":{"text":"2024-05-01T00:00:00","precision":"day"}}')
  expected <- switch(kind,
    float = literal,
    date = '{"date":{"text":"2024-05-01","precision":"day"}}',
    datetime = '{"datetime":{"text":"2024-05-01T00:00:00","precision":"second"}}')
  typed <- paste0('{"protocol":"dataset/1","functions":[{"identity":{"name":"identity","contract_version":"1","implementation_version":"2","call":"fixture.identity"},',
    '"parameters":[{"name":"value","host_name":"value","type":"', kind, '","accepts_missing":false,"presence":{"required":null}}],"returns":"', kind, '","may_return_missing":false}],',
    '"source":[{"name":"ID","kind":"int"},{"name":"A","kind":"int"},{"name":"B","kind":"int"}],"output":[{"name":"ID","kind":"int"},{"name":"V","kind":"', kind, '"}],',
    '"templates":[{"mode":{"records":null},"assignments":[{"column":0,"path":"columns.ID.source","expression":{"source":0}}]}],',
    '"columns":[{"column":1,"path":"columns.V.function","expression":{"function":{"slot":0,"arguments":[{"name":"value","input":{"literal":', literal, '}}]}}}],"keys":[0],"verifications":[]}')
  seen <- list()
  actual <- execute_dataset_functions(typed, source, list(), list(function(value) { seen[[length(seen) + 1L]] <<- value; value }))
  expected_snapshot <- paste0('{"protocol":"table/1","columns":[["ID","int"],["V","', kind, '"]],"row_count":"3","chunks":["3"],"rows":[',
    paste(vapply(1:3, function(id) paste0('[{"int":"', id, '"},', expected, ']'), ""), collapse = ","), ']}')
  stopifnot(identical(actual$outcome, truth$expected[1L]), identical(table_snapshot(actual$table), expected_snapshot), length(seen) == 3L)
  if (kind == "float") stopifnot(identical(paste(format(writeBin(seen[[1L]], raw(), size = 8L, endian = "big")), collapse = ""), "8000000000000000"))
  if (kind == "date") stopifnot(identical(seen[[1L]], as.Date("2024-05-01")))
  if (kind == "datetime") stopifnot(identical(as.numeric(seen[[1L]]), as.numeric(as.POSIXct("2024-05-01 00:00:00", tz = "UTC"))))
}
