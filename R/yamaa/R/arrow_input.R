# Internal in-memory counterpart of the prepared specification CSV probe.
# Tables can be exported repeatedly; nanoarrow/Arrow readers are consumed once.
.execute_specification_arrow <- function(handle, source) {
  .execute_specification_arrow_sources(handle, list(source))
}

.execute_specification_arrow_sources <- function(handle, sources) {
  if (!is.list(sources) || !identical(class(sources), "list")) {
    stop("Arrow sources must be an ordered list", call. = FALSE)
  }
  if (!requireNamespace("nanoarrow", quietly = TRUE)) {
    stop("Arrow C Data input requires the nanoarrow package", call. = FALSE)
  }
  failure <- NULL
  export <- function(index, destination) tryCatch({
    stream <- nanoarrow::as_nanoarrow_array_stream(sources[[index + 1L]])
    nanoarrow::nanoarrow_pointer_export(stream, destination)
    TRUE
  }, error = function(e) { failure <<- e; FALSE },
     interrupt = function(e) { failure <<- e; FALSE })
  reply <- .Call(wrap__execute_specification_arrow, handle, export,
                 as.integer(length(sources)))
  if (!is.null(failure)) stop(failure)
  if (!is.null(reply$error)) stop(reply$error, call. = FALSE)
  reply$value
}
