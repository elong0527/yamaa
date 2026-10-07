# Build tools may include Python, but the installed runtime probe must not see it.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("native-only-path-")
dir.create(runtime_path)
# R itself may invoke rm during temporary-file cleanup; preserve only that utility.
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaanative)
root <- system.file("specification-original", package="yamaanative", mustWork=TRUE)
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
module_names <- c("schema.yaml",sort(setdiff(list.files(file.path(root,"schema"),pattern="[.]yaml$"),"schema.yaml")))
modules <- setNames(lapply(file.path(root,"schema",module_names),rawfile),module_names)
for(case_name in c("negative-zero-division","negative-integer-overflow","adam-adlb-ordered-sum","schema-window-functions","schema-inheritance","schema-lookup")) {
  case <- file.path(root,"cases",case_name)
  specification <- if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml"
  if(case_name=="schema-inheritance") {
    inherited_prepare <- get(".prepare_inherited_specification",envir=asNamespace("yamaanative"))
    parent_reads <- character(); parent_resolutions <- character()
    canonicalize <- function(declaring,written) {
      parent_resolutions <<- c(parent_resolutions,written)
      candidate <- file.path(dirname(declaring),written)
      if(!file.exists(candidate)) return(NULL)
      c(normalizePath(candidate,winslash="/",mustWork=TRUE),candidate)
    }
    parent_capture <- function(identity,display_path,maximum) {
      parent_reads <<- c(parent_reads,basename(identity))
      bytes <- readBin(identity,"raw",n=maximum+1L)
      stopifnot(length(bytes)<=maximum)
      bytes
    }
    rebase <- function(layer,entry,written,maximum) {
      stopifnot(identical(dirname(layer),dirname(entry)))
      written
    }
    handle <- inherited_prepare(modules,"schema.yaml",normalizePath(file.path(case,specification),winslash="/"),rawfile(file.path(case,specification)),canonicalize,parent_capture,rebase)
    stopifnot(identical(parent_reads,c("spec_organization.yaml","spec_compound.yaml")),identical(parent_resolutions,c("spec_organization.yaml","spec_compound.yaml","spec_organization.yaml")))
  } else handle <- prepare_specification(modules,"schema.yaml",specification,rawfile(file.path(case,specification)))
  gc()
  inputs <- if(case_name=="schema-lookup") c(DM="input/dm.csv",AE="input/ae.csv",MEDDRA="input/meddict.csv") else if(case_name=="schema-window-functions") c(VS="input/vs.csv") else c(LB="input/lb.csv")
  stopifnot(identical(specification_source(handle),list(name=names(inputs)[[1L]],path=unname(inputs[[1L]]))))
  state <- new.env(parent=emptyenv()); state$reads <- 0L; state$requests <- character(); state$content <- list()
  capture <- function(name,path,maximum) {
    stopifnot(identical(path,unname(inputs[[name]])))
    state$requests <- c(state$requests,name)
    created <- is.null(state$content[[path]])
    if(created){
      state$content[[path]] <- readBin(file.path(case,path),"raw",n=maximum+1L)
      stopifnot(length(state$content[[path]])<=maximum)
      state$reads <- state$reads+1L
    }
    list(state$content[[path]],created)
  }
  expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  published <- 0L
  directory <- tempfile("original-published-"); dir.create(directory)
  publish <- function(path,content) {
    stopifnot(case_name %in% c("adam-adlb-ordered-sum","schema-window-functions","schema-inheritance","schema-lookup"),path==if(case_name=="schema-lookup") "adsl.csv" else if(case_name=="schema-window-functions") "advs.csv" else "adlb.csv")
    stopifnot(identical(content,rawfile(file.path(case,"expected",path))))
    pending <- file.path(directory,"candidate.csv")
    writeBin(content,pending)
    stopifnot(file.rename(pending,file.path(directory,path)))
    stopifnot(identical(rawfile(file.path(directory,path)),content))
    published <<- published+1L
    TRUE
  }
  for(created in c(1L,0L)) {
    report <- specification_report(handle,capture,publish,case_name,specification)
    if(created==0L) expected <- gsub('"snapshots_created":1','"snapshots_created":0',expected,fixed=TRUE)
    stopifnot(identical(report,expected))
  }
  stopifnot(state$reads==length(inputs),identical(state$requests,rep(names(inputs),2L)))
  stopifnot(published==if(case_name %in% c("adam-adlb-ordered-sum","schema-window-functions","schema-inheritance","schema-lookup")) 2L else 0L)
  unlink(directory,recursive=TRUE)
  cat(case_name,"complete original report and cached source capture passed\n")
  expired <- unserialize(serialize(handle,NULL))
  stopifnot(inherits(tryCatch(specification_source(expired),error=identity),"error"))
}
# The inherited preparation callbacks retain exact R errors and interrupts.
inherited_prepare <- get(".prepare_inherited_specification",envir=asNamespace("yamaanative"))
inherit_case <- file.path(root,"cases","schema-inheritance")
inherit_entry <- normalizePath(file.path(inherit_case,"spec_study.yaml"),winslash="/")
inherit_source <- rawfile(inherit_entry)
for(operation in seq_len(3L)) for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained inheritance condition",call=NULL,payload=new.env()),class=c("inheritance_test_condition",kind,"condition"))
  calls <- 0L
  callbacks <- list(
    function(declaring,written) {candidate<-file.path(dirname(declaring),written);c(normalizePath(candidate,winslash="/"),candidate)},
    function(identity,display_path,maximum) rawfile(identity),
    function(layer,entry,written,maximum) written)
  callbacks[[operation]] <- function(...) {calls <<- calls+1L;stop(failure)}
  actual <- tryCatch(inherited_prepare(modules,"schema.yaml",inherit_entry,inherit_source,callbacks[[1L]],callbacks[[2L]],callbacks[[3L]]),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),calls==1L)
}
for(cycle in c(FALSE,TRUE)) {
  canonicalize <- function(...) if(cycle) c(inherit_entry,inherit_entry) else NULL
  actual <- tryCatch(inherited_prepare(modules,"schema.yaml",inherit_entry,inherit_source,canonicalize,function(...) stop("unexpected source read"),function(...) stop("unexpected rebase")),error=identity)
  stopifnot(inherits(actual,"error"),grepl(if(cycle) '"condition":"inheritance_cycle"' else '"condition":"parent_not_found"',conditionMessage(actual),fixed=TRUE),grepl(if(cycle) 'REQ-0655' else 'REQ-0654',conditionMessage(actual),fixed=TRUE))
}
cat("inherited preparation callback errors, interrupts and shared diagnostics passed\n")
# The registered prototype single-buffer routine is not a public R wrapper.
# Its error envelope must still distinguish missing secondary inputs.
source_count <- .Call(get("wrap__execute_specification_csv",envir=asNamespace("yamaanative")),handle,rawfile(file.path(case,"input/dm.csv")))
stopifnot(identical(source_count$error,'{"outcome":{"code":"source_count","stage":"bind","status":"rejected"},"protocol":"specification/prototype"}'))
# The same condition object (including private payload identity) must cross the
# native call; neither errors nor interrupts may be converted into text or retried.
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained source condition",call=NULL,payload=new.env()),class=c("source_test_condition",kind,"condition"))
  calls <- 0L
  capture <- function(...) {calls <<- calls+1L;stop(failure)}
  actual <- tryCatch(specification_failure_report(handle,capture,"failure"),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),calls==1L)
}
# A later capture must retain prior observations and propagate the exact condition.
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained second source condition",call=NULL,payload=new.env()),class=c("source_test_condition",kind,"condition"))
  requests <- character()
  capture <- function(name,path,maximum) {
    requests <<- c(requests,name)
    if(name=="AE") stop(failure)
    list(rawfile(file.path(case,path)),TRUE)
  }
  publish <- function(...) stop("source failure reached publication")
  actual <- tryCatch(specification_report(handle,capture,publish,case_name),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),identical(requests,c("DM","AE")))
}
for(rejected in list(FALSE,NULL,NA,logical(),c(TRUE,FALSE),1L,"TRUE")) {
  calls <- 0L
  capture <- function(name,path,maximum) list(rawfile(file.path(case,path)),TRUE)
  publish <- function(...) {calls <<- calls+1L;rejected}
  actual <- tryCatch(specification_report(handle,capture,publish,case_name),error=identity)
  stopifnot(inherits(actual,"error"),calls==1L)
  stopifnot(identical(conditionMessage(actual),"publication callback rejected output"))
}
cat("rejected and malformed publication results passed\n")
# The last original case succeeds until the host publication boundary.
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained publication condition",call=NULL,payload=new.env()),class=c("publication_test_condition",kind,"condition"))
  calls <- 0L
  capture <- function(name,path,maximum) list(rawfile(file.path(case,path)),TRUE)
  publish <- function(...) {calls <<- calls+1L;stop(failure)}
  actual <- tryCatch(specification_report(handle,capture,publish,case_name),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),calls==1L)
}
for(invalid in list(NULL,1L,new("externalptr"))) {
  stopifnot(inherits(tryCatch(specification_source(invalid),error=identity),"error"))
}
cat("retained host errors/interruptions and invalid/expired handles passed\n")

Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
