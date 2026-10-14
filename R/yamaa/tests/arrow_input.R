# Optional C Stream transport contracts; ordinary package use needs no Arrow.
if (requireNamespace("nanoarrow", quietly = TRUE)) {
  library(yamaa)
  private <- function(name) get(name, asNamespace("yamaa"))
  prepare <- private(".prepare_document")
  execute <- private(".execute_specification_arrow")
  execute_sources <- private(".execute_specification_arrow_sources")
  no_io <- function(...) stop("in-memory execution entered file IO")
  spec <- charToRaw(paste0(
    "schema_version: '1.0'\ndomain: ARROW\nkeys: [ID]\n",
    "input: {SRC: unused.parquet}\n",
    "output: {path: unused.csv, columns: [ID, VALUE]}\n",
    "columns:\n  - {name: ID, type: str, label: Identifier, derivation: SRC.ID}\n",
    "  - {name: VALUE, type: int, label: Value, derivation: SRC.VALUE}\n"
  ))
  run <- prepare("memory.yaml", spec, no_io, no_io, no_io)
  source <- function() nanoarrow::basic_array_stream(list(
    data.frame(ID="a", VALUE=1L),
    data.frame(ID="b", VALUE=NA_integer_)
  ))
  stream <- source()
  result <- execute(run, stream)
  stopifnot(is.raw(result$table), grepl('"status":"success"', result$outcome, fixed=TRUE),
            !nanoarrow::nanoarrow_pointer_is_valid(stream))
  # Prepared handles can be reused, while moved streams cannot.
  actual <- tryCatch(execute(run, stream), error=identity)
  stopifnot(inherits(actual, "error"), identical(execute(run, source()), result))
  # Count admission must precede as_nanoarrow_array_stream() and any export.
  actual <- tryCatch(execute_sources(run, list(NULL, NULL)), error=identity)
  stopifnot(inherits(actual, "error"), grepl("source_count", conditionMessage(actual), fixed=TRUE))
  # Errors thrown by an exporter are rethrown as the original condition object.
  failure <- structure(list(message="opaque exporter error", call=NULL), class=c("arrow_export_error","error","condition"))
  as_nanoarrow_array_stream.yamaa_refuse <- function(x, ...) stop(failure)
  actual <- tryCatch(execute(run, structure(list(), class="yamaa_refuse")), error=identity)
  stopifnot(identical(actual, failure))
  # Unsupported schemas release the moved stream exactly once, before reading.
  released <- 0L
  bad <- nanoarrow::array_stream_set_finalizer(
    nanoarrow::basic_array_stream(list(data.frame(ID="a", VALUE=TRUE))),
    function() released <<- released + 1L
  )
  actual <- tryCatch(execute(run, bad), error=identity)
  stopifnot(inherits(actual, "error"), grepl("schema", conditionMessage(actual), fixed=TRUE),
            released == 1L, !nanoarrow::nanoarrow_pointer_is_valid(bad))
  # Foreign arrays and the stream also release on successful execution.
  good <- nanoarrow::array_stream_set_finalizer(source(), function() released <<- released + 1L)
  stopifnot(identical(execute(run, good), result), released == 2L)
  gc()
  stopifnot(released == 2L)
  if (requireNamespace("arrow", quietly = TRUE)) {
    table <- arrow::Table$create(ID=c("a", "b"), VALUE=c(1L, NA_integer_))
    stopifnot(identical(execute(run, table), result), identical(execute(run, table), result))
  }
  cat("Arrow C Stream input, lifetime, refusal and prepared-handle reuse passed\n")
}
