library(yamaa)

spec <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n",
  "input: {RAW: raw.csv}\n",
  "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\n",
  "output: {path: out.csv, columns: [ID]}\n")
objects <- list("spec.yaml" = charToRaw(spec), "raw.csv" = charToRaw("ID\n7\n"))
for (constructor in list(yamaa_memory_resolver, yamaa_synthetic_remote_resolver)) {
  resolver <- constructor(objects)
  result <- yamaa_domain("spec.yaml", resolver = resolver)
  stopifnot(nrow(result$issues) == 0L, identical(as.character(result$output$ID), "7"))
  stopifnot(any(vapply(result$resource_provenance, function(x) x$written == "raw.csv", logical(1))))
  resolver$backend$objects <- list()
  stopifnot(isTRUE(result$save()), length(resolver$backend$outputs) == 1L,
    identical(resolver$backend$outputs[[1L]], charToRaw("ID\n7\n")),
    length(resolver$backend$staging) == 0L)
}

nested_spec <- sub("raw.csv", "../raw.csv", spec, fixed = TRUE)
nested_spec <- sub("out.csv", "/dest/out.csv", nested_spec, fixed = TRUE)
resolver <- yamaa_memory_resolver(list("study/spec.yaml" = charToRaw(nested_spec),
  "raw.csv" = charToRaw("ID\n7\n")))
result <- yamaa_domain("study/spec.yaml", resolver = resolver)
stopifnot(nrow(result$issues) == 0L, isTRUE(result$save()),
  identical(resolver$backend$outputs[["memory:/dest/out.csv"]], charToRaw("ID\n7\n")))

# Reusing a stream input stores its arrays/schema, then creates a fresh stream
# for each capture, including repeated source aliases and later attempts.
frame_spec <- sub("input: \\{RAW: raw.csv\\}", "base: RAW\ninput: {RAW: raw.csv, SECOND: raw.csv}", spec)
resolver <- yamaa_memory_resolver(list("spec.yaml" = charToRaw(frame_spec)))
resolver$backend$put("raw.csv", NULL,
  frame = nanoarrow::as_nanoarrow_array_stream(data.frame(ID = c(7, 8))))
for (attempt in 1:2) {
  result <- yamaa_domain("spec.yaml", resolver = resolver)
  stopifnot(nrow(result$issues) == 0L, identical(as.character(result$output$ID), c("7", "8")))
}

missing <- yamaa_memory_resolver(list("spec.yaml" = charToRaw(spec)))
stopifnot(nrow(yamaa_check("spec.yaml", resolver = missing)$issues) == 0L)
result <- yamaa_domain("spec.yaml", resolver = missing)
stopifnot(is.null(result$output), "resource_path_missing" %in% result$issues$condition)

interruption <- structure(list(message = "synthetic interrupt", call = NULL),
  class = c("interrupt", "condition"))
source_resolver <- yamaa_memory_resolver(objects)
resolver <- yamaa_resource_resolver(function(request) {
  if (request$operation == "commit") stop(interruption)
  source_resolver$request(request)
})
result <- yamaa_domain("spec.yaml", resolver = resolver)
observed <- tryCatch(result$save(), interrupt = identity)
stopifnot(identical(observed, interruption), length(source_resolver$backend$staging) == 0L,
  length(source_resolver$backend$outputs) == 0L)

producer <- sub("raw.csv", "../raw/data.csv", spec, fixed = TRUE)
producer <- sub("out.csv", "../out/producer.csv", producer, fixed = TRUE)
consumer <- sub("RAW: raw.csv", "RAW: {path: ../out/producer.csv, schema: ../producer/root.yaml}", spec, fixed = TRUE)
consumer <- sub("out.csv", "../out/consumer.csv", consumer, fixed = TRUE)
resolver <- yamaa_synthetic_remote_resolver(list("consumer/root.yaml" = charToRaw(consumer),
  "producer/root.yaml" = charToRaw(producer), "raw/data.csv" = charToRaw("ID\n7\n")))
result <- yamaa_domain("consumer/root.yaml", resolver = resolver)
stopifnot(nrow(result$issues) == 0L, isTRUE(result$save()),
  identical(resolver$backend$outputs[["object://synthetic/out/producer.csv"]], charToRaw("ID\n7\n")),
  identical(resolver$backend$outputs[["object://synthetic/out/consumer.csv"]], charToRaw("ID\n7\n")))
