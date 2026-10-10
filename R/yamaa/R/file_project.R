# Private candidate project lifecycle. Raw closures and path authority are
# explicit; whole typed attempts stay native and do not grant a save capability.
.project_reply <- function(reply) {
  if (!is.null(reply$error)) stop(reply$error, call.=FALSE)
  reply$value
}
.prepare_file_project <- function(project_root, base_directory, entry, environment,
                                  modules, environment_modules, data_roots=character()) {
  if (!is.character(data_roots) || anyNA(data_roots) || !is.null(attributes(data_roots)) || length(data_roots)>=64L)
    stop("resource roots must be unclassed non-missing character paths",call.=FALSE)
  spec <- .specification_inputs(modules,"schema.yaml","",raw())
  env <- .specification_inputs(environment_modules,"schema_environment.yaml","",raw())
  # The native SDK accepts each explicit root first. Reordering bytes is only
  # schema argument marshalling; the shared compiler resolves the closure.
  order_root <- function(args) {
    order <- c(args$entry+1L,setdiff(seq_along(args$modules),args$entry+1L))
    list(names=args$names[order],modules=args$modules[order])
  }
  spec <- order_root(spec); env <- order_root(env)
  paths <- lapply(list(project_root,base_directory,entry,environment),.specification_text_bytes,maximum=65536)
  roots <- lapply(data_roots,.specification_text_bytes,maximum=65536)
  .project_reply(.Call(wrap__prepare_file_project,paths,roots,spec$names,spec$modules,env$names,env$modules))
}
.file_project_status <- function(handle) .project_reply(.Call(wrap__file_project_status,handle))
.file_project_reads <- function(handle) .project_reply(.Call(wrap__file_project_reads,handle))
.file_project_build <- function(handle) {
  capabilities <- .yamaa_locked_host_capabilities()
  .project_reply(.Call(wrap__file_project_build,handle,capabilities$verify,capabilities$resolve))
}
.file_project_observations <- function(handle) .project_reply(.Call(wrap__file_project_observations,handle))

# Formatting is explicitly limited to ordinary host conditions; interrupts stay
# rooted in the native attempt and propagate only after Rust returns.
.project_condition_details <- function(condition) {
  list(.function_detail(function() class(condition)[1L],"R condition"),
       .function_detail(function() conditionMessage(condition),"unavailable R condition message"))
}
.file_project_result <- function(attempt,metadata) {
  if(!is.list(metadata) || length(metadata)!=5L) stop("invalid report metadata",call.=FALSE)
  metadata <- lapply(metadata,.specification_text_bytes,maximum=4096)
  .project_reply(.Call(wrap__file_project_result,attempt,metadata,.project_condition_details))
}
.project_result_status <- function(handle) .project_reply(.Call(wrap__project_result_status,handle))
.project_result_output <- function(handle) .project_reply(.Call(wrap__project_result_output,handle))
.project_result_observations <- function(handle) .project_reply(.Call(wrap__project_result_observations,handle))
.project_result_issues <- function(handle) .project_reply(.Call(wrap__project_result_issues,handle))
.project_result_retained <- function(handle) .project_reply(.Call(wrap__project_result_retained,handle))
.project_result_propagate_interrupt <- function(handle) {
  condition <- .project_reply(.Call(wrap__project_result_interrupt,handle))
  if(!is.null(condition)) stop(condition)
  invisible(NULL)
}
.project_result_save <- function(handle,publish) .project_reply(.Call(wrap__project_result_save,handle,publish))
.project_result_save_file <- function(handle,publisher) .project_reply(.Call(wrap__project_result_save_file,handle,publisher))
