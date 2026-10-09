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
