# Private installed graph qualification. These handles expose retained evidence
# without granting a publisher or Save capability.
.prepare_producer_file <- function(specification, environment) {
  .project_reply(.Call(wrap__prepare_producer_file,
    .specification_text_bytes(specification,maximum=65536),
    .specification_text_bytes(environment,maximum=65536)))
}
.producer_status <- function(handle) .project_reply(.Call(wrap__producer_status,handle))
.producer_build <- function(handle,metadata,report_bytes=16777216L) {
  if(!is.list(metadata) || length(metadata)!=5L) stop("invalid report metadata",call.=FALSE)
  fields <- lapply(metadata,.specification_text_bytes,maximum=4096)
  capabilities <- .yamaa_locked_host_capabilities()
  .project_reply(.Call(wrap__producer_build,handle,fields,capabilities$verify,capabilities$resolve,as.integer(report_bytes)))
}
.producer_condition_details <- function(condition) {
  # Failed, oversized or reentrant rendering keeps the original native boundary.
  list(.specification_text_bytes(class(condition)[1L],maximum=65536),
       .specification_text_bytes(conditionMessage(condition),maximum=65536))
}
.producer_report <- function(handle,maximum=16777216L) .project_reply(.Call(wrap__producer_report,handle,.producer_condition_details,as.integer(maximum)))
.producer_observations <- function(handle) .project_reply(.Call(wrap__producer_observations,handle))
.producer_retained <- function(handle) .project_reply(.Call(wrap__producer_retained,handle))
.producer_artifact <- function(handle,node) .project_reply(.Call(wrap__producer_artifact,handle,as.integer(node)))
.producer_rejected_sources <- function(handle) .project_reply(.Call(wrap__producer_rejected_sources,handle))
.producer_rejection_issues <- function(handle,maximum=16777216L) .project_reply(.Call(wrap__producer_rejection_issues,handle,as.integer(maximum)))
.producer_propagate_interrupt <- function(handle) {
  condition <- .project_reply(.Call(wrap__producer_interrupt,handle))
  if(!is.null(condition)) stop(condition)
  invisible(NULL)
}
