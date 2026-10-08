# Private byte publication to one absolute file explicitly selected by the caller.
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
