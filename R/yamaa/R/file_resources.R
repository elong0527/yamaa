# Private filesystem byte transport. Rust owns path authority and immutable captures.
.file_resources_ports <- function(project_root, base_directory=project_root, data_roots=character()) {
  root <- .specification_text_bytes(project_root,65536)
  base <- .specification_text_bytes(base_directory,65536)
  if(!is.character(data_roots) || anyNA(data_roots) || !is.null(attributes(data_roots)) || length(data_roots)>=64L) {
    stop("data_roots must be unclassed character paths",call.=FALSE)
  }
  roots <- lapply(data_roots,.specification_text_bytes,maximum=65536)
  result <- .Call(wrap__create_file_resources,root,base,roots)
  if(!is.null(result$error)) stop(result$error,call.=FALSE)
  handle <- result$value
  reply <- function(result) {
    if(!is.null(result$error)) stop(result$error,call.=FALSE)
    value <- result$value
    if(is.list(value) && !is.null(value$kind)) {
      return(list(value$kind,simpleError(value$message)))
    }
    value
  }
  list(handle=handle,
    inspect=function(name,path) reply(.Call(wrap__inspect_file_resource,handle,.specification_text_bytes(path,65536))),
    capture=function(name,path,maximum) {
      if(!(typeof(maximum) %in% c("integer","double")) || length(maximum)!=1L || !is.null(attributes(maximum)) ||
          is.na(maximum) || !is.finite(maximum) || maximum<0 || maximum!=floor(maximum) || maximum>.Machine$integer.max) {
        stop("resource byte ceiling must be a bounded nonnegative integer",call.=FALSE)
      }
      reply(.Call(wrap__capture_file_resource,handle,.specification_text_bytes(path,65536),as.integer(maximum)))
    },
    reads=function() reply(.Call(wrap__file_resource_reads,handle)))
}
