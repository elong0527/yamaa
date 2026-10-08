library(yamaa)
truth <- read.delim(system.file("numeric_syntax.tsv", package = "yamaa"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) stopifnot(identical(analyze_numeric(truth$request[i]), truth$expected[i]))
}
for (request in list(NA_character_, character(), c("a", "b"), 1, NULL,
  structure("{}", class = "marked"),
  '{"protocol":"numeric-syntax/1","expression":null}',
  '{"protocol":"numeric-syntax/1","expression":"A","extra":1}',
  '{"protocol":"numeric-syntax/1","expression":"A","expression":"B"}',
  '{"protocol":"other","expression":"A"}')) {
  stopifnot(inherits(tryCatch(analyze_numeric(request), error = identity), "error"))
}
request <- paste0('{"protocol":"numeric-syntax/1","expression":"',
  strrep("A", 65537L), '"}')
stopifnot(identical(analyze_numeric(request),
  '{"outcome":{"limit":65536,"position":{"byte":0,"character":0},"resource":"bytes","status":"resource_limit"},"protocol":"numeric-syntax/1"}'))
stopifnot(identical(analyze_numeric(truth$request[1]), truth$expected[1]))
stopifnot(identical(engine_info()$execution_supported, FALSE))
