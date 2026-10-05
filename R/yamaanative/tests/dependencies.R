library(yamaanative)
truth <- read.delim(system.file("dependency_analysis.tsv", package = "yamaanative"),
                    sep = "\t", quote = "", comment.char = "", colClasses = "character",
                    fileEncoding = "UTF-8", check.names = FALSE)
for (i in seq_len(nrow(truth))) {
  graph <- vector("list", as.integer(truth$nodes[i]))
  if (truth$edges[i] != "-") {
    for (entry in strsplit(truth$edges[i], ";", fixed = TRUE)[[1L]]) {
      parts <- strsplit(entry, ":", fixed = TRUE)[[1L]]
      graph[[as.integer(parts[1L]) + 1L]] <- as.integer(strsplit(parts[2L], ",", fixed = TRUE)[[1L]])
    }
  }
  request <- paste0('{"protocol":"dependency-analysis/1","dependencies":[',
    paste(vapply(graph, function(edges) paste0("[", paste(edges, collapse = ","), "]"), ""), collapse = ","), "]}")
  cycle <- if (truth$cycle[i] == "-") "null" else paste0("[", truth$cycle[i], "]")
  order <- if (truth$order[i] == "-") "[]" else paste0("[", truth$order[i], "]")
  expected <- paste0('{"protocol":"dependency-analysis/1","outcome":{"status":"complete","cycle":',
                     cycle, ',"order":', order, "}}")
  for (attempt in 1:2) stopifnot(identical(analyze_dependencies(request), expected))
}
for (invalid in list(NA_character_, character(), c("a", "b"), 1, NULL,
  '{"protocol":"dependency-analysis/1","dependencies":[[1]]}',
  '{"protocol":"dependency-analysis/1","dependencies":[[true]]}')) {
  stopifnot(inherits(tryCatch(analyze_dependencies(invalid), error = identity), "error"))
}
request <- paste0('{"protocol":"dependency-analysis/1","dependencies":[',
                   paste(rep("[]", 4097L), collapse = ","), "]}")
stopifnot(identical(analyze_dependencies(request),
  '{"protocol":"dependency-analysis/1","outcome":{"status":"limit","resource":"nodes","limit":"4096","required":"4097"}}'))
stopifnot(identical(engine_info()$execution_supported, FALSE))
