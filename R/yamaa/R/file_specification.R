# Private native file lifecycle; all schema/model/source semantics live in Rust.
.prepare_file_specification <- function(project_root, base_directory, entry, data_roots=character()) {
  if (!is.character(data_roots) || anyNA(data_roots) || !is.null(attributes(data_roots)) || length(data_roots)>=64L)
    stop("resource roots must be unclassed non-missing character paths",call.=FALSE)
  root <- .specification_text_bytes(project_root,65536)
  base <- .specification_text_bytes(base_directory,65536)
  entry <- .specification_text_bytes(entry,65536)
  roots <- lapply(data_roots,.specification_text_bytes,maximum=65536)
  result <- .Call(wrap__prepare_file_specification,root,base,entry,roots)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.file_specification_source <- function(handle) {
  result <- .Call(wrap__file_specification_source,handle)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.file_specification_reads <- function(handle) {
  result <- .Call(wrap__file_specification_reads,handle)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.file_specification_check <- function(handle) {
  result <- .Call(wrap__file_specification_check,handle)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.file_specification_build <- function(handle,example,specification="spec.yaml",base_directory=".") {
  metadata <- lapply(list(as.character(getRversion()),engine_info()$core_version,
      example,specification,base_directory),.specification_text_bytes,maximum=4096)
  result <- .Call(wrap__file_specification_build,handle,metadata)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
