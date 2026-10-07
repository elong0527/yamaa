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
# Both hosts receive already-resolved context from the captured Rust schema.
prepare_entry <- get(".prepare_document",envir=asNamespace("yamaanative"))
no_port <- function(...) stop("invalid entry reached source authority")
preflight_truth <- read.delim(file.path(root,"preflight.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(preflight_truth)==5L)
for(i in seq_len(nrow(preflight_truth))) {
  failure <- tryCatch(prepare_entry("spec.yaml",charToRaw(preflight_truth$source[[i]]),no_port,no_port,no_port),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),preflight_truth$expected[[i]]))
}
cat("core preflight complete independent findings before ports passed\n")
for(literal in c("true","123456789012345678901234567890","null")) {
  failure <- tryCatch(prepare_entry("spec.yaml",charToRaw(paste0("schema_version: ",literal)),no_port,no_port,no_port),error=identity)
  expected <- paste0('{"outcome":{"diagnostics":[{"condition":"schema_version_mismatch","context":{"actual":',literal,',"expected":"1.0"},"phase":"validation","requirement":null,"spec_paths":["schema_version"]}],"status":"invalid"},"protocol":"specification/prototype"}')
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),expected))
}
decode_truth <- read.delim(file.path(root,"decode-replay.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(decode_truth)==8L)
for(i in seq_len(nrow(decode_truth))) {
  hex <- decode_truth$source_hex[[i]]
  starts <- seq.int(1L,nchar(hex),by=2L)
  bytes <- as.raw(strtoi(substring(hex,starts,starts+1L),base=16L))
  failure <- tryCatch(prepare_entry("source.yaml",bytes,no_port,no_port,no_port),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),decode_truth$expected[[i]]))
}
for(case_name in c("negative-zero-division","negative-integer-overflow","adam-adlb-ordered-sum","schema-window-functions","schema-inheritance","schema-lookup")) {
  case <- file.path(root,"cases",case_name)
  specification <- if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml"
  if(case_name=="schema-inheritance") {
    inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaanative"))
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
    handle <- inherited_prepare(normalizePath(file.path(case,specification),winslash="/"),rawfile(file.path(case,specification)),canonicalize,parent_capture,rebase)
    stopifnot(identical(parent_reads,c("spec_organization.yaml","spec_compound.yaml")),identical(parent_resolutions,c("spec_organization.yaml","spec_compound.yaml","spec_organization.yaml")))
  } else {
    no_parent <- function(...) stop("standalone preparation invoked a parent port")
    handle <- get(".prepare_document",envir=asNamespace("yamaanative"))(
      specification,rawfile(file.path(case,specification)),no_parent,no_parent,no_parent)
  }
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
  # Result construction does not publish; later saves retain bytes and never read.
  build <- get(".specification_build",envir=asNamespace("yamaanative"))
  build_output <- get(".build_output",envir=asNamespace("yamaanative"))
  build_observations <- get(".build_observations",envir=asNamespace("yamaanative"))
  build_save <- get(".build_save",envir=asNamespace("yamaanative"))
  before <- published
  result <- build(handle,capture,case_name,specification)
  requests_after_build <- state$requests
  unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
  stopifnot(identical(build_observations(result),unsaved),published==before)
  if(case_name %in% c("negative-zero-division","negative-integer-overflow")) {
    stopifnot(is.null(build_output(result)))
    failed_save <- tryCatch(build_save(result,function(...) stop("failed result reached publisher")),error=identity)
    stopifnot(inherits(failed_save,"error"),identical(conditionMessage(failed_save),"cannot save a failed build"))
  } else {
    bytes <- build_output(result)
    stopifnot(is.raw(bytes),length(bytes)>0L)
    altered <- bytes; altered[[1L]] <- as.raw(0L)
    stopifnot(identical(build_output(result),bytes))
    for(kind in c("error","interrupt")) {
      failure <- structure(list(message="retained save condition",call=NULL,payload=new.env()),class=c("save_test_condition",kind,"condition"))
      calls <- 0L
      actual <- tryCatch(build_save(result,function(...) {calls <<- calls+1L;stop(failure)}),error=identity,interrupt=identity)
      stopifnot(identical(actual,failure),calls==1L,identical(build_observations(result),unsaved))
    }
    for(i in seq_len(2L)) stopifnot(identical(build_save(result,publish),expected))
    stopifnot(published==before+2L)
  }
  stopifnot(identical(state$requests,requests_after_build))
  expired_result <- unserialize(serialize(result,NULL))
  stopifnot(inherits(tryCatch(build_output(expired_result),error=identity),"error"))
  stopifnot(inherits(tryCatch(build_output(handle),error=identity),"error"))
  stopifnot(inherits(tryCatch(specification_source(result),error=identity),"error"))
  unlink(directory,recursive=TRUE)
  cat(case_name,"complete original report and cached source capture passed\n")
  expired <- unserialize(serialize(handle,NULL))
  stopifnot(inherits(tryCatch(specification_source(expired),error=identity),"error"))
}
# Replay existing complete graph failure truth through the raw-YAML entry point.
inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaanative"))
replay <- read.delim(file.path(root,"inheritance-replay.tsv"),sep="\t",quote="",comment.char="",colClasses="character",check.names=FALSE,fileEncoding="UTF-8")
stopifnot(length(unique(replay$case))==7L)
for(name in unique(replay$case)) {
  rows <- replay[replay$case==name,,drop=FALSE]
  expected_calls <- rows[nzchar(rows$operation),,drop=FALSE]
  calls <- 0L
  event <- function(operation) {
    calls <<- calls+1L
    stopifnot(calls<=nrow(expected_calls),identical(operation,expected_calls$operation[[calls]]))
    expected_calls[calls,,drop=FALSE]
  }
  canonicalize <- function(declaring,written) {
    row <- event("canonicalize")
    stopifnot(identical(declaring,row$declaring[[1L]]),identical(written,row$written[[1L]]))
    if(!nzchar(row$identity[[1L]])) return(NULL)
    c(row$identity[[1L]],row$display_path[[1L]])
  }
  parent_capture <- function(identity,display_path,maximum) {
    row <- event("read")
    stopifnot(identical(identity,row$identity[[1L]]),identical(display_path,row$display_path[[1L]]))
    if(!nzchar(row$source_yaml[[1L]])) return(NULL)
    bytes <- charToRaw(row$source_yaml[[1L]])
    stopifnot(length(bytes)<=maximum)
    bytes
  }
  actual <- tryCatch(inherited_prepare(rows$entry[[1L]],charToRaw(rows$entry_yaml[[1L]]),canonicalize,parent_capture,function(...) stop("failed traversal reached rebasing")),error=identity)
  stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),rows$expected[[1L]]),calls==nrow(expected_calls))
}
cat("seven raw inherited-loader failure contracts and complete traces passed\n")
preparation_truth <- read.delim(file.path(root,"inheritance-preparation.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(preparation_truth)==3L)
for(i in seq_len(nrow(preparation_truth))) {
  counts <- c(0L,0L,0L)
  canonicalize <- function(declaring,written) {
    stopifnot(identical(declaring,"entry.yaml"),identical(written,"parent.yaml"))
    counts[[1L]] <<- counts[[1L]]+1L
    c("parent.yaml","parent.yaml")
  }
  parent_capture <- function(identity,display_path,maximum) {
    stopifnot(identical(identity,"parent.yaml"),identical(display_path,"parent.yaml"))
    counts[[2L]] <<- counts[[2L]]+1L
    bytes <- charToRaw(preparation_truth$parent_yaml[[i]])
    stopifnot(length(bytes)<=maximum)
    bytes
  }
  rebase <- function(layer,entry,written,maximum) {
    counts[[3L]] <<- counts[[3L]]+1L
    written
  }
  failure <- tryCatch(inherited_prepare("entry.yaml",charToRaw(preparation_truth$entry_yaml[[i]]),canonicalize,parent_capture,rebase),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),preparation_truth$expected[[i]]),identical(counts,c(1L,1L,as.integer(preparation_truth$rebases[[i]]))))
}
cat("inherited layer, composition and dependency issue records passed\n")
# The inherited preparation callbacks retain exact R errors and interrupts.
inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaanative"))
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
  actual <- tryCatch(inherited_prepare(inherit_entry,inherit_source,callbacks[[1L]],callbacks[[2L]],callbacks[[3L]]),error=identity,interrupt=identity)
  stopifnot(identical(actual,failure),calls==1L)
}
for(cycle in c(FALSE,TRUE)) {
  canonicalize <- function(...) if(cycle) c(inherit_entry,inherit_entry) else NULL
  actual <- tryCatch(inherited_prepare(inherit_entry,inherit_source,canonicalize,function(...) stop("unexpected source read"),function(...) stop("unexpected rebase")),error=identity)
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
  calls <- 0L
  actual <- tryCatch(build(handle,capture,"failure"),error=identity,interrupt=identity)
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

# A container-only variation preserves original values and native observations.
# R builds and saves with no Python runtime or host Parquet library available.
parquet_case <- file.path(root,"cases","adam-adlb-ordered-sum")
parquet_source <- sub("path: adlb.csv","path: adlb.parquet",
                      rawToChar(rawfile(file.path(parquet_case,"spec.yaml"))),fixed=TRUE)
stopifnot(grepl("path: adlb.parquet",parquet_source,fixed=TRUE))
parquet_handle <- prepare_entry("spec.yaml",charToRaw(parquet_source),no_port,no_port,no_port)
parquet_requests <- character()
parquet_capture <- function(name,path,maximum) {
  parquet_requests <<- c(parquet_requests,path)
  bytes <- rawfile(file.path(parquet_case,path))
  stopifnot(length(bytes)<=maximum)
  list(bytes,TRUE)
}
parquet_result <- build(parquet_handle,parquet_capture,"adam-adlb-ordered-sum","spec.yaml")
parquet_expected <- rawToChar(rawfile(file.path(root,"expected","adam-adlb-ordered-sum.json")))
parquet_expected <- sub('"runtime":"python"','"runtime":"r"',parquet_expected,fixed=TRUE)
parquet_expected <- sub("fixture-runtime",as.character(getRversion()),parquet_expected,fixed=TRUE)
parquet_expected <- sub("fixture-engine",engine_info()$core_version,parquet_expected,fixed=TRUE)
parquet_unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',parquet_expected)
stopifnot(identical(build_observations(parquet_result),parquet_unsaved))
parquet_saved <- list()
parquet_publish <- function(path,bytes) {
  stopifnot(path=="adlb.parquet",identical(bytes[1:4],charToRaw("PAR1")),
            identical(tail(bytes,4L),charToRaw("PAR1")))
  parquet_saved[[length(parquet_saved)+1L]] <<- bytes
  TRUE
}
for(i in seq_len(2L)) {
  parquet_report <- build_save(parquet_result,parquet_publish)
  stopifnot(grepl('"profile":"parquet"',parquet_report,fixed=TRUE),
            grepl('"content":""',parquet_report,fixed=TRUE),
            identical(sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',parquet_report),parquet_unsaved))
}
stopifnot(identical(parquet_requests,"input/lb.csv"),length(parquet_saved)==2L,
          identical(parquet_saved[[1L]],parquet_saved[[2L]]),
          identical(build_observations(parquet_result),parquet_unsaved),
          !is.null(build_output(parquet_result)))
cat("native Parquet output construction and explicit save passed\n")

# Only the source container changes. The complete original report and exact CSV
# remain independent truth. The runtime PATH still contains no Python executable.
case_name <- "adam-adlb-ordered-sum"
case <- file.path(root,"cases",case_name)
source <- sub("path: input/lb.csv, types: {LBSTRESN: float}","path: input/lb.parquet",
              rawToChar(rawfile(file.path(case,"spec.yaml"))),fixed=TRUE)
handle <- prepare_entry("spec.yaml",charToRaw(source),no_port,no_port,no_port)
expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
expected <- gsub('"path":"input/lb.csv"','"path":"input/lb.parquet"',expected,fixed=TRUE)
requests <- character(); saves <- 0L
capture <- function(name,path,maximum) {
  stopifnot(name=="LB",path=="input/lb.parquet")
  requests <<- c(requests,path)
  content <- rawfile(file.path(root,"pq","ordered-sum.parquet"))
  stopifnot(length(content)<=maximum)
  list(content,TRUE)
}
result <- build(handle,capture,case_name)
unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
stopifnot(identical(build_observations(result),unsaved),!is.null(build_output(result)))
rm(handle);gc()
for(i in seq_len(2L)) {
  report <- build_save(result,function(path,content) {
    stopifnot(path=="adlb.csv",identical(content,rawfile(file.path(case,"expected","adlb.csv"))))
    saves <<- saves+1L
    TRUE
  })
  stopifnot(identical(report,expected),identical(build_observations(result),unsaved))
}
stopifnot(identical(requests,"input/lb.parquet"),saves==2L)
redundant <- sub("input/lb.csv","input/lb.parquet",rawToChar(rawfile(file.path(case,"spec.yaml"))),fixed=TRUE)
failure <- tryCatch(prepare_entry("spec.yaml",charToRaw(redundant),no_port,no_port,no_port),error=identity)
stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),
  '{"outcome":{"diagnostics":[{"condition":"redundant_field_type","context":{"dataset":"LB","field":"LBSTRESN","type":"float"},"phase":"validation","requirement":"REQ-0533","spec_paths":["input.LB.types.LBSTRESN"]}],"status":"invalid"},"protocol":"specification/prototype"}'))
cat("native Parquet source complete original report, exact CSV and preflight passed\n")

# Independent codec failures retain a complete failed report and deny save.
source <- '{"schema_version":"1.0","domain":"TEST","input":{"SRC":{"path":"input.parquet"}},"keys":["ID"],"columns":[{"name":"ID","type":"int","derivation":{"compute":{"expr":"1"}}}],"output":{"path":"output.csv","columns":["ID"]}}'
handle <- prepare_entry("spec.yaml",charToRaw(source),no_port,no_port,no_port)
cases <- list(
  c("utf8","source_parquet_invalid","REQ-1038",'"dataset":"SRC","path":"input.parquet"'),
  c("utf8-bool","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"OTHER","path":"input.parquet","stored_type":"bool"'),
  c("utf8-time","source_field_value_invalid","REQ-1041",'"dataset":"SRC","field":"OTHER","path":"input.parquet","row":1,"value":1'),
  c("empty-name","source_field_name_empty","REQ-1039",'"dataset":"SRC","field":1,"path":"input.parquet"'),
  c("duplicate","source_field_name_duplicate","REQ-1039",'"dataset":"SRC","field":"I","path":"input.parquet"'),
  c("mixed-struct","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"S","path":"input.parquet","stored_type":"struct<left: int64, right: string>"'),
  c("mixed-map","source_field_type_unsupported","REQ-1040",'"dataset":"SRC","field":"M","path":"input.parquet","stored_type":"map<string, int64 (\'M\')>"'),
  c("mixed-empty","source_field_name_empty","REQ-1039",'"dataset":"SRC","field":1,"path":"input.parquet"'),
  c("mixed-duplicate","source_field_name_duplicate","REQ-1039",'"dataset":"SRC","field":"I","path":"input.parquet"')
)
unsupported_types <- c(int32="int32",uint64="uint64",float32="float",binary="binary",
  milliseconds="timestamp[ms]",timezone="timestamp[us, tz=UTC]",list="list<element: int64>",
  "fixed-list"="fixed_size_list<element: int64>[2]",struct="struct<item: int64>",decimal="decimal128(10, 2)")
for(name in names(unsupported_types)) cases[[length(cases)+1L]] <- c(
  paste0("unsupported-",name),"source_field_type_unsupported","REQ-1040",
  paste0('"dataset":"SRC","field":"FIELD","path":"input.parquet","stored_type":"',unsupported_types[[name]],'"'))
for(test in cases) {
  requests <- character()
  capture <- function(name,path,maximum) {
    stopifnot(name=="SRC",path=="input.parquet")
    requests <<- c(requests,path)
    content <- rawfile(file.path(root,"pq",paste0(test[[1L]],".parquet")))
    stopifnot(length(content)<=maximum)
    list(content,TRUE)
  }
  result <- build(handle,capture,test[[1L]])
  finding <- paste0('{"condition":"',test[[2L]],'","context":{',test[[4L]],'},"phase":"ingest","requirement":"',test[[3L]],'","spec_paths":["input.SRC.path"]}')
  expected <- paste0('{"artifacts":[],"backend":"rust","callbacks":[],"diagnostics":[',finding,'],"engine_version":"',engine_info()$core_version,'","error":null,"example":"',test[[1L]],'","handler_counts":[],"nodes":[{"diagnostics":[',finding,'],"handler_counts":[],"outcome":"failure","specification":"spec.yaml","unsupported":[]}],"outcome":"failure","report_version":"0.3.0-draft","runtime":"r","runtime_version":"',as.character(getRversion()),'","source_reads":[{"base_directory":".","condition":null,"outcome":"captured","path":"input.parquet","snapshots_created":1}],"tables":[],"unsupported":[],"verifications":[]}')
  stopifnot(identical(build_observations(result),expected),is.null(build_output(result)))
  failure <- tryCatch(build_save(result,no_port),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"),identical(requests,"input.parquet"))
}
cat("native Parquet source complete failed reports and save gates passed\n")

# Input policy remains core-owned and retained output distinguishes missing text
# from present empty text after the preparation handle has been released.
for(policy in c("missing","present")) {
  source <- paste0('{"schema_version":"1.0","domain":"TEST","input":{"SRC":{"path":"input.PARQUET","empty_string":"',policy,'"}},"keys":["I"],"columns":[{"name":"I","type":"int","derivation":"SRC.I"},{"name":"S","type":"str","derivation":"SRC.S"}],"output":{"path":"output.csv","columns":["I","S"]}}')
  handle <- prepare_entry("spec.yaml",charToRaw(source),no_port,no_port,no_port)
  requests <- character(); saves <- 0L
  capture <- function(name,path,maximum) {
    stopifnot(name=="SRC",path=="input.PARQUET")
    requests <<- c(requests,path)
    content <- rawfile(file.path(root,"pq","text.parquet"))
    stopifnot(length(content)<=maximum)
    list(content,TRUE)
  }
  result <- build(handle,capture,policy)
  rm(handle);gc()
  expected <- charToRaw(if(policy=="missing") "I,S\n1,\n2,X\n" else 'I,S\n1,""\n2,X\n')
  for(i in seq_len(2L)) {
    report <- build_save(result,function(path,content) {
      stopifnot(path=="output.csv",identical(content,expected))
      saves <<- saves+1L
      TRUE
    })
    stopifnot(grepl('"outcome":"success"',report,fixed=TRUE))
  }
  stopifnot(identical(requests,"input.PARQUET"),saves==2L)
}
cat("native Parquet empty-string input policy and retained save passed\n")

# Semantic failures collect through the final source in declaration order; a
# failed input collection exposes no partial tables or publication capability.
source <- '{"schema_version":"1.0","domain":"TEST","base":"FIRST","input":{"FIRST":{"path":"FIRST.parquet"},"SECOND":{"path":"SECOND.parquet"},"THIRD":{"path":"THIRD.parquet"}},"keys":["ID"],"columns":[{"name":"ID","type":"int","derivation":{"compute":{"expr":"1"}}}],"output":{"path":"output.csv","columns":["ID"]}}'
handle <- prepare_entry("spec.yaml",charToRaw(source),no_port,no_port,no_port)
requests <- character()
capture <- function(name,path,maximum) {
  stopifnot(path==paste0(name,".parquet"))
  requests <<- c(requests,name)
  fixture <- c(FIRST="utf8-time",SECOND="duplicate",THIRD="text")[[name]]
  content <- rawfile(file.path(root,"pq",paste0(fixture,".parquet")))
  stopifnot(length(content)<=maximum)
  list(content,TRUE)
}
result <- build(handle,capture,"collection")
findings <- paste0(
  '{"condition":"source_field_value_invalid","context":{"dataset":"FIRST","field":"OTHER","path":"FIRST.parquet","row":1,"value":1},"phase":"ingest","requirement":"REQ-1041","spec_paths":["input.FIRST.path"]},',
  '{"condition":"source_field_name_duplicate","context":{"dataset":"SECOND","field":"I","path":"SECOND.parquet"},"phase":"ingest","requirement":"REQ-1039","spec_paths":["input.SECOND.path"]}')
reads <- paste0('{"base_directory":".","condition":null,"outcome":"captured","path":"',c("FIRST","SECOND","THIRD"),'.parquet","snapshots_created":1}',collapse=",")
expected <- paste0('{"artifacts":[],"backend":"rust","callbacks":[],"diagnostics":[',findings,'],"engine_version":"',engine_info()$core_version,'","error":null,"example":"collection","handler_counts":[],"nodes":[{"diagnostics":[',findings,'],"handler_counts":[],"outcome":"failure","specification":"spec.yaml","unsupported":[]}],"outcome":"failure","report_version":"0.3.0-draft","runtime":"r","runtime_version":"',as.character(getRversion()),'","source_reads":[',reads,'],"tables":[],"unsupported":[],"verifications":[]}')
stopifnot(identical(build_observations(result),expected),is.null(build_output(result)),
          identical(requests,c("FIRST","SECOND","THIRD")))
failure <- tryCatch(build_save(result,no_port),error=identity)
stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"))
cat("native Parquet source failure collection and complete report passed\n")

failed_report_truth <- function(fixture,prefix,cases,content=charToRaw("ID\n1\n")) {
  truth <- read.delim(file.path(root,fixture),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
  stopifnot(nrow(truth)==cases)
  for(i in seq_len(nrow(truth))) {
    held <- content
    if("source_hex" %in% names(truth)) {
      hex <- truth$source_hex[[i]]
      held <- if(nzchar(hex)) as.raw(strtoi(substring(hex,seq.int(1L,nchar(hex),2L),seq.int(2L,nchar(hex),2L)),16L)) else raw()
    }
    handle <- prepare_entry("spec.yaml",charToRaw(truth$source[[i]]),no_port,no_port,no_port)
    reads <- character()
    capture <- function(dataset,path,maximum) {
      stopifnot(dataset=="SRC",path=="source.csv",maximum>=length(held))
      reads <<- c(reads,path)
      list(held,TRUE)
    }
    result <- build(handle,capture,paste0(prefix,"-",truth$case[[i]]),"spec.yaml")
    rm(handle); gc()
    expected <- gsub('"runtime":"python"','"runtime":"r"',truth$expected[[i]],fixed=TRUE)
    expected <- gsub('"runtime_version":"fixture-runtime"',paste0('"runtime_version":"',as.character(getRversion()),'"'),expected,fixed=TRUE)
    expected <- gsub('"engine_version":"fixture-engine"',paste0('"engine_version":"',engine_info()$core_version,'"'),expected,fixed=TRUE)
    stopifnot(identical(build_observations(result),expected),is.null(build_output(result)))
    for(j in seq_len(2L)) {
      failure <- tryCatch(build_save(result,no_port),error=identity)
      stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"),
                identical(build_observations(result),expected))
    }
    stopifnot(identical(reads,"source.csv"))
  }
}
failed_report_truth("output-declarations.tsv","output",5L)
cat("core output declarations complete independent failed reports and retained save gates passed\n")
failed_report_truth("grammar-diagnostics.tsv","grammar",7L)
cat("core grammar complete independent failed reports and retained save gates passed\n")
failed_report_truth("binding-diagnostics.tsv","binding",10L,charToRaw("ID,V\n1,2\n"))
cat("core binding complete independent failed reports and retained save gates passed\n")
failed_report_truth("window-diagnostics.tsv","window",3L,charToRaw("ID,V\n1,2\n"))
cat("core window complete independent failed reports and retained save gates passed\n")

failed_report_truth("csv-profile-diagnostics.tsv","csv",13L)
cat("core CSV profile complete independent failed reports and retained save gates passed\n")

scalar_report_truth <- function(fixture,prefix,cases) {
  truth <- read.delim(file.path(root,fixture),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="UTF-8",check.names=FALSE)
  stopifnot(nrow(truth)==cases)
  for(i in seq_len(nrow(truth))) {
    handle <- prepare_entry("spec.yaml",charToRaw(truth$source[[i]]),no_port,no_port,no_port)
    reads <- character(); saves <- 0L
    capture <- function(dataset,path,maximum) {
      stopifnot(dataset=="SRC",path=="source.csv",maximum>=5L)
      reads <<- c(reads,path)
      list(charToRaw("ID\n1\n"),TRUE)
    }
    result <- build(handle,capture,paste0(prefix,"-",truth$case[[i]]),"spec.yaml")
    rm(handle);gc()
    expected <- gsub('"runtime":"python"','"runtime":"r"',truth$expected[[i]],fixed=TRUE)
    expected <- gsub('"runtime_version":"fixture-runtime"',paste0('"runtime_version":"',as.character(getRversion()),'"'),expected,fixed=TRUE)
    expected <- gsub('"engine_version":"fixture-engine"',paste0('"engine_version":"',engine_info()$core_version,'"'),expected,fixed=TRUE)
    unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
    stopifnot(identical(build_observations(result),unsaved))
    hex <- truth$output_hex[[i]]
    if(nzchar(hex)) {
      exact <- as.raw(strtoi(substring(hex,seq.int(1L,nchar(hex),2L),seq.int(2L,nchar(hex),2L)),16L))
      stopifnot(!is.null(build_output(result)))
      for(j in seq_len(2L)) {
        report <- build_save(result,function(path,content) {
          stopifnot(path=="result.csv",identical(content,exact));saves <<- saves+1L;TRUE
        })
        stopifnot(identical(report,expected))
      }
      stopifnot(saves==2L)
    } else {
      stopifnot(is.null(build_output(result)))
      for(j in seq_len(2L)) {
        failure <- tryCatch(build_save(result,function(...) stop("failed literal build published")),error=identity)
        stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"))
      }
      stopifnot(saves==0L)
    }
    stopifnot(identical(build_observations(result),unsaved),identical(reads,"source.csv"))
  }
}
scalar_report_truth("column-literals.tsv","literal",4L)
cat("original column literals complete independent reports and exact CSV passed\n")
scalar_report_truth("original-conversion-handlers.tsv","handler",5L)
cat("original conversion handlers complete independent reports and exact CSV passed\n")
scalar_report_truth("original-row-conversion-handlers.tsv","row-handler",8L)
cat("original row conversion handlers complete independent reports and exact CSV passed\n")

Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
