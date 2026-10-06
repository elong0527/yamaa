# Independent canonical source traces and complete expected owned documents.
library(yamaanative)
schema <- "{\"protocol\":\"schema/1\",\"schema\":{\"entry\":0,\"modules\":[{\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"kind\":\"text\",\"value\":\"root_class\"},{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"type\"},{\"kind\":\"text\",\"value\":\"str\"},{\"entries\":[[4,5]],\"kind\":\"mapping\"},{\"entries\":[[3,6]],\"kind\":\"mapping\"},{\"kind\":\"text\",\"value\":\"parents\"},{\"kind\":\"text\",\"value\":\"type\"},{\"kind\":\"text\",\"value\":\"str\"},{\"kind\":\"text\",\"value\":\"list[str]\"},{\"items\":[10,11],\"kind\":\"sequence\"},{\"entries\":[[9,12]],\"kind\":\"mapping\"},{\"entries\":[[8,13]],\"kind\":\"mapping\"},{\"items\":[7,14],\"kind\":\"sequence\"},{\"entries\":[[0,1],[2,15]],\"kind\":\"mapping\"}],\"root\":16},\"name\":\"schema.yaml\"}],\"root_class\":\"root_class\"}}"
request <- "{\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"kind\":\"text\",\"value\":\"parents\"},{\"kind\":\"text\",\"value\":\"a\"},{\"items\":[3],\"kind\":\"sequence\"},{\"entries\":[[0,1],[2,4]],\"kind\":\"mapping\"}],\"root\":5},\"entry\":{\"display_path\":\"/entry\",\"identity\":\"/entry\"},\"protocol\":\"inheritance/1\"}"
empty <- "{\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"kind\":\"text\",\"value\":\"parents\"},{\"items\":[],\"kind\":\"sequence\"},{\"entries\":[[0,1],[2,3]],\"kind\":\"mapping\"}],\"root\":4},\"entry\":{\"display_path\":\"/entry\",\"identity\":\"/entry\"},\"protocol\":\"inheritance/1\"}"
empty_expected <- "{\"outcome\":{\"layers\":[{\"display_path\":\"/entry\",\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"kind\":\"text\",\"value\":\"parents\"},{\"items\":[],\"kind\":\"sequence\"},{\"entries\":[[0,1],[2,3]],\"kind\":\"mapping\"}],\"root\":4},\"identity\":\"/entry\"}],\"status\":\"traversed\"},\"protocol\":\"inheritance/1\"}"
expected <- "{\"outcome\":{\"layers\":[{\"display_path\":\"/a\",\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"entries\":[[0,1]],\"kind\":\"mapping\"}],\"root\":2},\"identity\":\"/a\"},{\"display_path\":\"/entry\",\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"kind\":\"text\",\"value\":\"parents\"},{\"kind\":\"text\",\"value\":\"a\"},{\"items\":[3],\"kind\":\"sequence\"},{\"entries\":[[0,1],[2,4]],\"kind\":\"mapping\"}],\"root\":5},\"identity\":\"/entry\"}],\"status\":\"traversed\"},\"protocol\":\"inheritance/1\"}"
resolve_request <- "{\"declaring\":\"/entry\",\"operation\":\"canonicalize\",\"path\":\"a\",\"protocol\":\"inheritance/1\"}"
read_request <- "{\"display_path\":\"/a\",\"identity\":\"/a\",\"operation\":\"read\",\"protocol\":\"inheritance/1\"}"
resolve_reply <- "{\"outcome\":{\"display_path\":\"/a\",\"identity\":\"/a\",\"status\":\"resolved\"},\"protocol\":\"inheritance/1\"}"
read_reply <- "{\"outcome\":{\"document\":{\"nodes\":[{\"kind\":\"text\",\"value\":\"schema_version\"},{\"kind\":\"text\",\"value\":\"1.0\"},{\"entries\":[[0,1]],\"kind\":\"mapping\"}],\"root\":2},\"status\":\"document\"},\"protocol\":\"inheritance/1\"}"

stopifnot(identical(
  traverse_inheritance(schema, empty, function(...) stop("unexpected source IO")),
  empty_expected
))
calls <- character()
allowance <- numeric()
callback <- function(message, maximum) {
  calls <<- c(calls, message)
  allowance <<- c(allowance, maximum)
  # Independent nested traversal must not share the outer graph state.
  stopifnot(identical(
    traverse_inheritance(schema, empty, function(...) stop("nested source IO")),
    empty_expected
  ))
  gc()
  if (length(calls) == 1L) {
    stopifnot(identical(message, resolve_request))
    return(resolve_reply)
  }
  stopifnot(length(calls) == 2L, identical(message, read_request))
  read_reply
}
stopifnot(identical(traverse_inheritance(schema, request, callback), expected))
stopifnot(identical(calls, c(resolve_request, read_request)),
          allowance[2L] < allowance[1L])
for (kind in c("error", "interrupt")) {
  original <- structure(
    list(message = "original source condition", call = NULL, marker = 73L),
    class = c("source_marker", kind, "condition")
  )
  caught <- tryCatch(
    traverse_inheritance(schema, request, function(...) stop(original)),
    error = identity, interrupt = identity
  )
  stopifnot(identical(caught, original))
}
bad <- tryCatch(
  traverse_inheritance(schema, request, function(...) NA_character_),
  error = identity
)
stopifnot(inherits(bad, "error"))
large <- tryCatch(
  traverse_inheritance(schema, request, function(message, maximum) {
    strrep(" ", maximum + 1L)
  }),
  error = identity
)
stopifnot(inherits(large, "error"),
          grepl("reply exceeds byte limit", conditionMessage(large), fixed = TRUE))
stopifnot(identical(
  traverse_inheritance(schema, empty, function(...) stop("unexpected recovery IO")),
  empty_expected
))
cat("Installed R inheritance traces, ownership, reentrancy and original conditions passed\n")

# Complete graph outcomes and source traces are independently authored and
# replayed unchanged by Rust and both installed host services.
load_truth <- function(name) read.delim(system.file(name, package = "yamaanative"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
truth <- load_truth("inheritance_traversal.tsv")
sources <- load_truth("inheritance_sources.tsv")
stopifnot(nrow(truth) == 8L)
for (i in seq_len(nrow(truth))) {
  expected_sources <- sources[sources$case == truth$case[i], , drop = FALSE]
  calls <- character()
  result <- traverse_inheritance(truth$schema[i], truth$request[i],
    function(message, maximum) {
      index <- length(calls) + 1L
      stopifnot(index <= nrow(expected_sources),
                identical(message, expected_sources$request[index]),
                nchar(expected_sources$reply[index], type = "bytes") <= maximum)
      calls <<- c(calls, message)
      expected_sources$reply[index]
    })
  stopifnot(identical(result, truth$expected[i]),
            identical(calls, expected_sources$request))
}
cat("Eight complete inheritance outcomes and source traces passed\n")
