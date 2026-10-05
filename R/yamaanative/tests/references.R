library(yamaanative)
truth <- read.delim(system.file("reference_binding.tsv", package = "yamaanative"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) stopifnot(identical(analyze_references(truth$request[i]), truth$expected[i]))
}
for (request in list(NA_character_, character(), c("a", "b"), 1, NULL,
  '{"protocol":"reference-analysis/1","catalog":{"outputs":[{"name":"X","type":"bool"}],"datasets":[]},"queries":[]}',
  '{"protocol":"reference-analysis/1","catalog":{"outputs":[{"name":"X","type":"int"},{"name":"X","type":"str"}],"datasets":[]},"queries":[]}',
  '{"protocol":"reference-analysis/1","catalog":{"outputs":[],"datasets":[]},"queries":[{"kind":"validate_output","name":"X","expected":null,"available":null,"candidates":[0]}]}')) {
  stopifnot(inherits(tryCatch(analyze_references(request), error = identity), "error"))
}
query <- '{"kind":"bind","name":"X"}'
request <- paste0('{"protocol":"reference-analysis/1","catalog":{"outputs":[],"datasets":[]},"queries":[',
  paste(rep(query, 4097L), collapse = ","), "]}")
stopifnot(identical(analyze_references(request),
  '{"protocol":"reference-analysis/1","outcome":{"status":"limit","resource":"queries","limit":"4096","required":"4097"}}'))
stopifnot(identical(analyze_references(truth$request[1L]), truth$expected[1L]))
stopifnot(identical(engine_info()$execution_supported, FALSE))
