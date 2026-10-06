library(yamaanative)
truth <- read.delim(system.file("predicate_syntax.tsv", package = "yamaanative"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) stopifnot(identical(analyze_predicate(truth$request[i]), truth$expected[i]))
}
for (request in list(NA_character_, character(), c("a", "b"), 1, NULL,
  structure("{}", class = "marked"), structure("{}", names = "request"),
  '{"protocol":"predicate-syntax/1","expression":null}',
  '{"protocol":"predicate-syntax/1","expression":"TRUE","extra":1}',
  '{"protocol":"predicate-syntax/1","expression":"TRUE","expression":"FALSE"}',
  '{"protocol":"predicate-syntax/1","expression":"\\ud800"}',
  '{"protocol":"other","expression":"TRUE"}', strrep(" ", 1048577L))) {
  stopifnot(inherits(tryCatch(analyze_predicate(request), error = identity), "error"))
}
request <- paste0('{"protocol":"predicate-syntax/1","expression":"',
  strrep(" ", 65537L), '"}')
stopifnot(identical(analyze_predicate(request),
  '{"outcome":{"limit":65536,"phase":"parse","position":{"byte":0,"character":0},"resource":"bytes","status":"resource_limit"},"protocol":"predicate-syntax/1"}'))
request <- paste0('{"protocol":"predicate-syntax/1","expression":"',
  "'", intToUtf8(0x1f600), "' = @", '"}')
stopifnot(identical(analyze_predicate(request),
  '{"outcome":{"condition":"invalid_predicate","context":{},"position":{"byte":9,"character":6},"requirement":"REQ-0188","status":"invalid"},"protocol":"predicate-syntax/1"}'))
stopifnot(identical(analyze_predicate(truth$request[1]), truth$expected[1]))
stopifnot(identical(engine_info()$execution_supported, FALSE))
