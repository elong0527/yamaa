library(yamaa)
truth <- read.delim(system.file("regex_transport.tsv", package = "yamaa"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
stopifnot(nrow(truth) == 38L)
for (i in seq_len(nrow(truth))) {
  for (attempt in 1:2) stopifnot(identical(evaluate_regex(truth$request[i]), truth$expected[i]))
}
for (request in list(NA_character_, character(), c("a", "b"), TRUE, 1, NULL,
  structure("{}", class = "marked"),
  '{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","subject":"a"}}',
  '{"protocol":"regex/1","pattern":"a","pattern":"b","operation":{"kind":"compile"}}',
  '{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":null}}',
  '{"protocol":"regex/1","pattern":"a","operation":{"kind":"search","subject":"a","extra":1}}',
  '{"protocol":"regex/1","pattern":"a","operation":{"kind":"compile","kind":"search"}}',
  '{"protocol":"other","pattern":"a","operation":{"kind":"compile"}}',
  strrep(" ", 1048577L))) {
  stopifnot(inherits(tryCatch(evaluate_regex(request), error = identity), "error"))
}
bad <- rawToChar(as.raw(c(0xc0, 0xaf)))
Encoding(bad) <- "UTF-8"
stopifnot(inherits(tryCatch(evaluate_regex(bad), error = identity), "error"))

# Authored Unicode results require no JSON library or Python installation.
for (value in c(intToUtf8(0x1d400), paste0("e", intToUtf8(0x301)),
  intToUtf8(0xfeff), intToUtf8(0x85))) {
  request <- paste0('{"protocol":"regex/1","pattern":"([^]*)",',
    '"operation":{"kind":"full_match","subject":"', value, '"}}')
  expected <- paste0('{"protocol":"regex/1","contract_version":"2.0.0",',
    '"outcome":{"status":"matched","group_count":1,"groups":["', value, '","', value, '"]}}')
  actual <- evaluate_regex(request)
  evaluate_regex(truth$request[1])
  stopifnot(identical(actual, expected))
}
request <- paste0('{"protocol":"regex/1","pattern":"a",',
  '"operation":{"kind":"search","subject":"', strrep("a", 500000L), '"}}')
stopifnot(identical(evaluate_regex(request), paste0(
  '{"protocol":"regex/1","contract_version":"2.0.0","outcome":',
  '{"status":"resource_limit","phase":"match","resource":"state_cells","limit":1000000}}')))
request <- paste0('{"protocol":"regex/1","pattern":"', strrep("(", 32L),
  '.{8192}', strrep(")", 32L), '","operation":{"kind":"full_match","subject":"',
  strrep("\\u0000", 8192L), '"}}')
stopifnot(identical(evaluate_regex(request), paste0(
  '{"protocol":"regex/1","contract_version":"2.0.0","outcome":',
  '{"status":"resource_limit","phase":"response","resource":"response_bytes","limit":1048576}}')))
stopifnot(identical(evaluate_regex(truth$request[1]), truth$expected[1]))
stopifnot(identical(engine_info()$execution_supported, FALSE))
