library(yamaanative)
# The same independently authored complete JSON truth is replayed by both hosts.
truth <- do.call(rbind, lapply(c("schema_model.tsv", "schema_transport.tsv", "schema_windows.tsv", "schema_composition.tsv", "schema_layer_admission.tsv", "schema_inheritance_dependencies.tsv"), function(name) {
  cases <- read.delim(system.file(name, package = "yamaanative"),
    sep = "\t", quote = "", comment.char = "", colClasses = "character",
    fileEncoding = "UTF-8", check.names = FALSE)
  expected_rows <- if (name %in% c("schema_model.tsv", "schema_layer_admission.tsv")) 8L else 6L
  stopifnot(nrow(cases) == expected_rows)
  cases
}))
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
