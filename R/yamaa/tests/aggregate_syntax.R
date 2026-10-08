library(yamaa)
truth <- read.delim(system.file("aggregate_syntax.tsv", package = "yamaa"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) stopifnot(identical(analyze_aggregate(truth$request[i]), truth$expected[i]))
}
for (request in list(NA_character_, character(), c("a", "b"), 1, NULL,
  structure("{}", class = "marked"),
  '{"protocol":"aggregate-syntax/1","expression":null}',
  '{"protocol":"aggregate-syntax/1","expression":"A","extra":1}',
  '{"protocol":"aggregate-syntax/1","expression":"A","expression":"B"}',
  '{"protocol":"other","expression":"A"}')) {
  stopifnot(inherits(tryCatch(analyze_aggregate(request), error = identity), "error"))
}
request <- paste0('{"protocol":"aggregate-syntax/1","expression":"',
  strrep("A", 65537L), '"}')
stopifnot(identical(analyze_aggregate(request),
  '{"outcome":{"limit":65536,"position":{"byte":0,"character":0},"resource":"bytes","status":"resource_limit"},"protocol":"aggregate-syntax/1"}'))
stopifnot(identical(analyze_aggregate(truth$request[1]), truth$expected[1]))
stopifnot(identical(engine_info()$execution_supported, FALSE))
