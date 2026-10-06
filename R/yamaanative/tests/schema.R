library(yamaanative)
# The same independently authored complete JSON truth is replayed by both hosts.
truth <- read.delim(system.file("schema_transport.tsv", package = "yamaanative"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) {
    stopifnot(identical(interpret_schema(truth$request[i]), truth$expected[i]))
  }
}
for (request in list(NA_character_, character(), c("a", "b"), 1, NULL,
  structure("{}", class = "marked"), structure("{}", names = "request"),
  '{}', '{"protocol":"schema/1","protocol":"schema/1"}',
  strrep(" ", 8388609L))) {
  stopifnot(inherits(tryCatch(interpret_schema(request), error = identity), "error"))
}
stopifnot(identical(interpret_schema(truth$request[1L]), truth$expected[1L]),
  identical(engine_info()$execution_supported, FALSE))
