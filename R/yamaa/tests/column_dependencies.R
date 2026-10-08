library(yamaa)
truth <- read.delim(system.file("column_dependencies.tsv", package = "yamaa"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
indices_json <- function(text) if (text %in% c("-", "_")) "[]" else paste0("[", text, "]")
metadata <- list(
  cycle = c("dependency_cycle", "REQ-0072", "operation"),
  forward = c("forward_reference", "REQ-0071", "operation"),
  missing = c("key_dependency", "REQ-0074", "declaration"),
  key = c("key_dependency", "REQ-0074", "expression")
)
for (i in seq_len(nrow(truth))) {
  graph <- if (truth$dependencies[i] == "~") character() else
    vapply(strsplit(truth$dependencies[i], ";", fixed = TRUE)[[1L]],
      function(text) if (text == "-") "null" else indices_json(text), "")
  diagnostics <- if (truth$diagnostics[i] == "-") character() else
    vapply(strsplit(truth$diagnostics[i], ";", fixed = TRUE)[[1L]], function(entry) {
      fields <- strsplit(entry, ":", fixed = TRUE)[[1L]]
      meta <- metadata[[fields[1L]]]
      paste0('{"condition":"', meta[1L], '","requirement":"', meta[2L],
        '","location":"', meta[3L], '","columns":', indices_json(fields[2L]), "}")
    }, "")
  request <- paste0('{"protocol":"column-dependencies/1","dependencies":[',
    paste(graph, collapse = ","), '],"keys":', indices_json(truth$keys[i]),
    ',"has_rows":', truth$has_rows[i], "}")
  expected <- paste0('{"protocol":"column-dependencies/1","outcome":{"status":"complete","order":',
    indices_json(truth$order[i]), ',"diagnostics":[', paste(diagnostics, collapse = ","), "]}}")
  for (attempt in 1:2) stopifnot(identical(analyze_column_dependencies(request), expected))
}
for (invalid in list(NA_character_, character(), c("a", "b"), 1, NULL,
  '{"protocol":"column-dependencies/1","dependencies":[[]],"keys":[0,0],"has_rows":false}',
  '{"protocol":"column-dependencies/1","dependencies":[[]],"keys":[1],"has_rows":false}',
  '{"protocol":"column-dependencies/1","dependencies":[[1]],"keys":[],"has_rows":true}',
  '{"protocol":"column-dependencies/1","dependencies":[[true]],"keys":[],"has_rows":true}',
  '{"protocol":"column-dependencies/1","dependencies":[],"keys":[],"has_rows":0}')) {
  stopifnot(inherits(tryCatch(analyze_column_dependencies(invalid), error = identity), "error"))
}
request <- paste0('{"protocol":"column-dependencies/1","dependencies":[',
  paste(rep("null", 4097L), collapse = ","), '],"keys":[],"has_rows":true}')
stopifnot(identical(analyze_column_dependencies(request),
  '{"protocol":"column-dependencies/1","outcome":{"status":"limit","resource":"nodes","limit":"4096","required":"4097"}}'))
stopifnot(identical(analyze_column_dependencies(
  '{"protocol":"column-dependencies/1","dependencies":[[]],"keys":[0],"has_rows":false}'),
  '{"protocol":"column-dependencies/1","outcome":{"status":"complete","order":[0],"diagnostics":[]}}'))
stopifnot(identical(engine_info()$execution_supported, FALSE))
