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
for(case_name in c("negative-zero-division","negative-integer-overflow","adam-adlb-ordered-sum")) {
  case <- file.path(root,"cases",case_name)
  handle <- prepare_specification(modules,"schema.yaml","spec.yaml",rawfile(file.path(case,"spec.yaml")))
  gc()
  stopifnot(identical(specification_source(handle),list(name="LB",path="input/lb.csv")))
  state <- new.env(parent=emptyenv()); state$reads <- 0L; state$requests <- 0L; state$content <- NULL
  capture <- function(name,path,maximum) {
    stopifnot(name=="LB",path=="input/lb.csv")
    state$requests <- state$requests+1L
    created <- is.null(state$content)
    if(created){
      state$content <- readBin(file.path(case,path),"raw",n=maximum+1L)
      stopifnot(length(state$content)<=maximum)
      state$reads <- state$reads+1L
    }
    list(state$content,created)
  }
  expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  published <- 0L
  directory <- tempfile("original-published-"); dir.create(directory)
  publish <- function(path,content) {
    stopifnot(case_name=="adam-adlb-ordered-sum",path=="adlb.csv")
    stopifnot(identical(content,rawfile(file.path(case,"expected",path))))
    pending <- file.path(directory,"candidate.csv")
    writeBin(content,pending)
    stopifnot(file.rename(pending,file.path(directory,path)))
    stopifnot(identical(rawfile(file.path(directory,path)),content))
    published <<- published+1L
  }
  for(created in c(1L,0L)) {
    report <- specification_report(handle,capture,publish,case_name)
    if(created==0L) expected <- sub('"snapshots_created":1','"snapshots_created":0',expected,fixed=TRUE)
    stopifnot(identical(report,expected))
  }
  stopifnot(state$reads==1L,state$requests==2L)
  stopifnot(published==if(case_name=="adam-adlb-ordered-sum") 2L else 0L)
  unlink(directory,recursive=TRUE)
  cat(case_name,"complete original report and cached source capture passed\n")
  expired <- unserialize(serialize(handle,NULL))
  stopifnot(inherits(tryCatch(specification_source(expired),error=identity),"error"))
}
# The same condition object (including private payload identity) must cross the
# native call; neither errors nor interrupts may be converted into text or retried.
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained source condition",call=NULL,payload=new.env()),class=c("source_test_condition",kind,"condition"))
  calls <- 0L
  capture <- function(...) {calls <<- calls+1L;stop(failure)}
  actual <- tryCatch(specification_failure_report(handle,capture,"failure"),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),calls==1L)
}
# The last original case succeeds until the host publication boundary.
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="retained publication condition",call=NULL,payload=new.env()),class=c("publication_test_condition",kind,"condition"))
  calls <- 0L
  capture <- function(...) list(rawfile(file.path(case,"input/lb.csv")),TRUE)
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
