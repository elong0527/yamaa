# The Arrow view is optional. The native result remains the owner of the held
# build; each request transfers a fresh stream cursor to nanoarrow's finalizer.
.domain_arrow <- function(handle, kind) {
  if (!requireNamespace("nanoarrow", quietly = TRUE) ||
      utils::packageVersion("nanoarrow") < "0.8.0") {
    stop("Arrow views require nanoarrow >= 0.8.0", call. = FALSE)
  }
  destination <- nanoarrow::nanoarrow_allocate_array_stream()
  .domain_reply(.native_domain_arrow(handle, destination, kind))
}
