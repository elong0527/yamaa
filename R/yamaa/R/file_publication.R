#' Private native file publisher
#'
#' Available on the qualified Unix hosts. `target` is the absolute file explicitly
#' selected by the caller; `declared_path` is the specification spelling every
#' publication request must match. Read-root authority does not select this file.
#' `publish` replaces it with bounded raw bytes; `save` publishes an owned native
#' build result through its failed-build gate without reading study data again.
#' @param target One absolute UTF-8 file target.
#' @param declared_path One declared UTF-8 artifact path.
#' @return A registered native handle with `publish(path, content)` and
#'   `save(result_handle)` closures.
#' @keywords internal
.file_publisher_port <- function(target, declared_path) {
  target <- .specification_text_bytes(target,65536)
  declared <- .specification_text_bytes(declared_path,65536)
  result <- .Call(wrap__create_file_publisher,declared,target)
  if(!is.null(result$error)) stop(result$error,call.=FALSE)
  handle <- result$value
  list(handle=handle,publish=function(path,content) {
    path <- .specification_text_bytes(path,65536)
    if(!is.raw(content) || !is.null(attributes(content)) || length(content)>67108864) {
      stop("publication content must be bounded unclassed raw bytes",call.=FALSE)
    }
    result <- .Call(wrap__publish_file_artifact,handle,path,content)
    if(!is.null(result$error)) stop(result$error,call.=FALSE)
    result$value
  },save=function(result_handle) {
    result <- .Call(wrap__save_build_file_artifact,result_handle,handle)
    if(!is.null(result$error)) stop(result$error,call.=FALSE)
    result$value
  })
}
