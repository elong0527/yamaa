# Build tools may include Python, but the installed runtime probe must not see it.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("native-only-path-")
dir.create(runtime_path)
# R itself may invoke rm during temporary-file cleanup; preserve only that utility.
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
root <- system.file("specification-original", package="yamaa", mustWork=TRUE)
source(file.path(root,"issue-frame-truth.R"),local=TRUE)
check_issue_frame <- function(actual,expected) {
  stopifnot(is.data.frame(actual),identical(names(actual),c("phase","condition","requirement","spec_paths","context")))
  stopifnot(nrow(actual)==length(expected$phase),identical(lapply(actual,identity),expected))
  stopifnot(all(vapply(actual[c("phase","condition","requirement","context")],is.character,logical(1))),is.list(actual$spec_paths))
}
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
module_names <- c("schema.yaml",sort(setdiff(list.files(file.path(root,"schema"),pattern="[.]yaml$"),"schema.yaml")))
modules <- setNames(lapply(file.path(root,"schema",module_names),rawfile),module_names)
# Both hosts receive already-resolved context from the captured Rust schema.
prepare_entry <- get(".prepare_document",envir=asNamespace("yamaa"))
no_port <- function(...) stop("invalid entry reached source authority")
unknown_profile <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n",
  "input: {SRC: input.unknown}\noutput: {path: out.csv, columns: [ID]}\n",
  "columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n")
unknown_failure <- tryCatch(prepare_entry("spec.yaml",charToRaw(unknown_profile),no_port,no_port,no_port),error=identity)
stopifnot(inherits(unknown_failure,"error"),identical(conditionMessage(unknown_failure),
  '{"outcome":{"diagnostics":[{"condition":"source_profile_unknown","context":{"dataset":"SRC","path":"input.unknown"},"phase":"validation","requirement":"REQ-0852","spec_paths":["input.SRC.path"]}],"status":"invalid"},"protocol":"specification/prototype"}'))
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
for(case_name in c("negative-zero-division","negative-integer-overflow","adam-adlb-ordered-sum","schema-window-functions","schema-inheritance","schema-lookup","negative-formula-flag","negative-row-aggregate","negative-row-no-prior","negative-source-missing-field","negative-source-trivial-filter","negative-paired-dates","negative-not-missing-age","negative-implausible-age","negative-invalid-sex","negative-sex-code","negative-matches-bad-pattern")) {
  case <- file.path(root,"cases",case_name)
  specification <- if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml"
  if(case_name=="schema-inheritance") {
    inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaa"))
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
    handle <- get(".prepare_document",envir=asNamespace("yamaa"))(
      specification,rawfile(file.path(case,specification)),no_parent,no_parent,no_parent)
  }
  gc()
  inputs <- if(case_name=="schema-lookup") c(DM="input/dm.csv",AE="input/ae.csv",MEDDRA="input/meddict.csv") else if(case_name %in% c("schema-window-functions","negative-row-no-prior")) c(VS="input/vs.csv") else if(case_name %in% c("negative-source-missing-field","negative-source-trivial-filter")) c(ODM="input/odm.csv") else if(case_name %in% c("negative-paired-dates","negative-not-missing-age","negative-implausible-age","negative-invalid-sex","negative-sex-code","negative-matches-bad-pattern")) c(DM="input/dm.csv") else c(LB="input/lb.csv")
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
  build <- get(".specification_build",envir=asNamespace("yamaa"))
  build_output <- get(".build_output",envir=asNamespace("yamaa"))
  build_observations <- get(".build_observations",envir=asNamespace("yamaa"))
  build_save <- get(".build_save",envir=asNamespace("yamaa"))
  before <- published
  result <- build(handle,capture,case_name,specification)
  requests_after_build <- state$requests
  build_issues <- get(".build_issues",envir=asNamespace("yamaa"))
  for(i in seq_len(2L)) check_issue_frame(build_issues(result),issue_frame_truth[[case_name]])
  copied_issues <- build_issues(result)
  if(nrow(copied_issues)) copied_issues$phase[[1L]] <- "caller mutation"
  check_issue_frame(build_issues(result),issue_frame_truth[[case_name]])
  unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
  stopifnot(identical(build_observations(result),unsaved),published==before)
  if(startsWith(case_name,"negative-")) {
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
  cat(case_name,"owned build issue data frame exact fields, context and cached independence passed\n")
  expired_result <- unserialize(serialize(result,NULL))
  stopifnot(inherits(tryCatch(build_output(expired_result),error=identity),"error"))
  stopifnot(inherits(tryCatch(build_issues(expired_result),error=identity),"error"))
  stopifnot(inherits(tryCatch(build_output(handle),error=identity),"error"))
  stopifnot(inherits(tryCatch(build_issues(handle),error=identity),"error"))
  stopifnot(inherits(tryCatch(specification_source(result),error=identity),"error"))
  # Exercise native filesystem transport over every unchanged original input.
  ports <- get(".file_resources_ports",envir=asNamespace("yamaa"))(case,case)
  native_expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  native_expected <- sub('"runtime":"python"','"runtime":"r"',native_expected,fixed=TRUE)
  native_expected <- sub('fixture-runtime',as.character(getRversion()),native_expected,fixed=TRUE)
  native_expected <- sub('fixture-engine',engine_info()$core_version,native_expected,fixed=TRUE)
  for(created in c(1L,0L)) {
    if(created==0L) native_expected <- gsub('"snapshots_created":1','"snapshots_created":0',native_expected,fixed=TRUE)
    actual <- specification_report(handle,ports$capture,publish,case_name,specification,inspect=ports$inspect)
    stopifnot(identical(actual,native_expected))
  }
  stopifnot(ports$reads()==length(inputs))
  cat(case_name,"native file source complete report and cached direct bytes passed\n")
  # Save held successful bytes through native explicit-target publication.
  target_name <- if(case_name=="schema-lookup") "adsl.csv" else if(case_name=="schema-window-functions") "advs.csv" else "adlb.csv"
  target <- file.path(directory,target_name)
  publisher <- get(".file_publisher_port",envir=asNamespace("yamaa"))(target,target_name)
  native_result <- build(handle,ports$capture,case_name,specification,inspect=ports$inspect)
  before_reads <- ports$reads()
  if(startsWith(case_name,"negative-")) {
    actual <- tryCatch(publisher$save(native_result),error=identity)
    stopifnot(inherits(actual,"error"),conditionMessage(actual)=="cannot save a failed build",!file.exists(target))
  } else {
    for(j in seq_len(2L)) stopifnot(identical(publisher$save(native_result),native_expected))
    stopifnot(identical(rawfile(target),rawfile(file.path(case,"expected",target_name))))
  }
  stopifnot(ports$reads()==before_reads)
  cat(case_name,"native file publication complete reports, exact bytes and retained save gates passed\n")
  # Preparation, checking and build require no R semantic model or source callback.
  file_handle <- get(".prepare_file_specification",envir=asNamespace("yamaa"))(case,case,specification)
  file_reads <- get(".file_specification_reads",envir=asNamespace("yamaa"))
  initial_reads <- if(case_name=="schema-inheritance") 3L else 1L
  stopifnot(file_reads(file_handle)==initial_reads)
  file_check <- get(".file_specification_check",envir=asNamespace("yamaa"))
  stopifnot(identical(file_check(file_handle),get(".specification_check_issues",envir=asNamespace("yamaa"))(handle)),file_reads(file_handle)==initial_reads)
  file_expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  file_expected <- sub('"runtime":"python"','"runtime":"r"',file_expected,fixed=TRUE)
  file_expected <- sub('fixture-runtime',as.character(getRversion()),file_expected,fixed=TRUE)
  file_expected <- sub('fixture-engine',engine_info()$core_version,file_expected,fixed=TRUE)
  for(created in c(1L,0L)) {
    if(created==0L) file_expected <- gsub('"snapshots_created":1','"snapshots_created":0',file_expected,fixed=TRUE)
    file_result <- get(".file_specification_build",envir=asNamespace("yamaa"))(file_handle,case_name,specification)
    file_unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',file_expected)
    stopifnot(identical(build_observations(file_result),file_unsaved),file_reads(file_handle)==initial_reads+length(inputs))
    if(startsWith(case_name,"negative-")) {
      actual <- tryCatch(publisher$save(file_result),error=identity)
      stopifnot(inherits(actual,"error"),conditionMessage(actual)=="cannot save a failed build",!file.exists(target))
    } else {
      for(j in seq_len(2L)) stopifnot(identical(publisher$save(file_result),file_expected))
      stopifnot(identical(rawfile(target),rawfile(file.path(case,"expected",target_name))))
    }
    stopifnot(file_reads(file_handle)==initial_reads+length(inputs))
  }
  expired_file <- unserialize(serialize(file_handle,NULL))
  stopifnot(inherits(tryCatch(file_check(expired_file),error=identity),"error"),inherits(tryCatch(file_check(handle),error=identity),"error"))
  cat(case_name,"native file preparation complete reports, cached build and direct saves without host ports passed\n")
  unlink(directory,recursive=TRUE)
  cat(case_name,"complete original report and cached source capture passed\n")
  expired <- unserialize(serialize(handle,NULL))
  stopifnot(inherits(tryCatch(specification_source(expired),error=identity),"error"))
}
# Replay existing complete graph failure truth through the raw-YAML entry point.
inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaa"))
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
inherited_prepare <- get(".prepare_document",envir=asNamespace("yamaa"))
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
# The following boundary probes use an explicit successful lookup fixture,
# independent of the preceding corpus loop order.
case_name <- "schema-lookup"
case <- file.path(root,"cases",case_name)
handle <- prepare_entry("spec.yaml",rawfile(file.path(case,"spec.yaml")),no_port,no_port,no_port)
# The registered prototype single-buffer routine is not a public R wrapper.
# Its error envelope must still distinguish missing secondary inputs.
source_count <- .Call(get("wrap__execute_specification_csv",envir=asNamespace("yamaa")),handle,rawfile(file.path(case,"input/dm.csv")))
stopifnot(identical(source_count$error,'{"outcome":{"code":"source_count","stage":"bind","status":"rejected"},"protocol":"specification/prototype"}'))
# Explicit resource replies preserve the common port's complete observations.
capture_truth <- read.delim(file.path(root,"source-capture.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(capture_truth)==8L)
for(i in seq_len(nrow(capture_truth))) {
  row <- capture_truth[i,,drop=FALSE]
  fail_at <- as.integer(row$fail_at[[1L]])
  cached <- identical(row$cached[[1L]],"1")
  failure <- structure(list(message="opaque filesystem detail",call=NULL,payload=new.env()),class=c("port_test_error","error","condition"))
  requests <- character()
  capture <- function(name,path,maximum) {
    index <- length(requests)
    requests <<- c(requests,name)
    if(index==fail_at) return(list(row$kind[[1L]],failure))
    content <- rawfile(file.path(case,path))
    stopifnot(length(content)<=maximum)
    list(content,!cached)
  }
  expected <- row$expected[[1L]]
  expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  result <- build(handle,capture,case_name)
  expected_requests <- c("DM","AE")[seq_len(fail_at+1L)]
  stopifnot(identical(requests,expected_requests),identical(build_observations(result),expected),is.null(build_output(result)))
  requests <- character()
  stopifnot(identical(specification_failure_report(handle,capture,case_name),expected),identical(requests,expected_requests))
  requests <- character()
  stopifnot(identical(specification_report(handle,capture,function(...) stop("failed capture published"),case_name),expected),identical(requests,expected_requests))
  rm(capture,failure); gc()
  for(j in seq_len(2L)) {
    actual <- tryCatch(build_save(result,function(...) stop("failed result reached publisher")),error=identity)
    stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),"cannot save a failed build"))
  }
  stopifnot(identical(build_observations(result),expected))
}
cat("classified source replies complete failed reports, cached counters and retained save gates passed\n")
for(reply in list(list("unknown",simpleError("opaque")),list("missing",FALSE),list("not_regular_file",new.env()),list("missing"))) {
  calls <- 0L
  capture <- function(...) {calls <<- calls+1L;reply}
  actual <- tryCatch(build(handle,capture,case_name),error=identity)
  stopifnot(inherits(actual,"error"),calls==1L)
}
failure <- structure(list(message="returned interrupt",call=NULL,payload=new.env()),class=c("port_test_interrupt","interrupt","condition"))
actual <- tryCatch(build(handle,function(...) list("missing",failure),case_name),error=identity,interrupt=identity)
stopifnot(identical(actual,failure))
cat("source reply transport validation and original returned interrupt passed\n")

# Metadata inspection collects known causes before any byte capture.
inspection_truth <- read.delim(file.path(root,"source-inspection.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(inspection_truth)==7L)
inspection_cases <- list(missing_first=c(DM="missing"),missing_later=c(AE="missing"),directory_first=c(DM="not_regular_file"),directory_later=c(AE="not_regular_file"),mixed=c(DM="missing",AE="not_regular_file"),reverse_mixed=c(DM="not_regular_file",AE="missing"),all_missing=c(DM="missing",AE="missing",MEDDRA="missing"))
for(i in seq_len(nrow(inspection_truth))) {
  row <- inspection_truth[i,,drop=FALSE]
  causes <- inspection_cases[[row$case[[1L]]]]
  failure <- structure(list(message="private inspection payload",call=NULL,payload=new.env()),class=c("inspection_test_error","error","condition"))
  requests <- character()
  inspect <- function(name,path) {
    requests <<- c(requests,name)
    stopifnot(identical(path,unname(c(DM="input/dm.csv",AE="input/ae.csv",MEDDRA="input/meddict.csv")[[name]])))
    if(name %in% names(causes)) return(list(unname(causes[[name]]),failure))
    NULL
  }
  capture <- function(...) stop("metadata failure reached study capture")
  expected <- row$expected[[1L]]
  expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  for(j in seq_len(2L)) {
    requests <- character()
    result <- build(handle,capture,"schema-lookup",inspect=inspect)
    stopifnot(identical(requests,c("DM","AE","MEDDRA")),identical(build_observations(result),expected),is.null(build_output(result)))
    actual <- tryCatch(build_save(result,function(...) stop("inspection failure published")),error=identity)
    stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),"cannot save a failed build"))
  }
  requests <- character()
  stopifnot(identical(specification_failure_report(handle,capture,"schema-lookup",inspect=inspect),expected),identical(requests,c("DM","AE","MEDDRA")))
  requests <- character()
  stopifnot(identical(specification_report(handle,capture,function(...) stop("inspection failure published"),"schema-lookup",inspect=inspect),expected),identical(requests,c("DM","AE","MEDDRA")))
  # Construct the same independently authored metadata causes using real files.
  directory <- tempfile("native-inspection-"); dir.create(directory)
  dir.create(file.path(directory,"input"))
  paths <- c(DM="input/dm.csv",AE="input/ae.csv",MEDDRA="input/meddict.csv")
  for(name in names(paths)) {
    path <- file.path(directory,paths[[name]])
    if(name %in% names(causes)) {
      if(causes[[name]]=="not_regular_file") dir.create(path)
    } else writeBin(charToRaw("bytes that must remain unread"),path)
  }
  ports <- get(".file_resources_ports",envir=asNamespace("yamaa"))(directory,directory)
  for(j in seq_len(2L)) {
    result <- build(handle,ports$capture,"schema-lookup",inspect=ports$inspect)
    stopifnot(identical(build_observations(result),expected),is.null(build_output(result)),ports$reads()==0L)
    actual <- tryCatch(build_save(result,function(...) stop("native inspection failure published")),error=identity)
    stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),"cannot save a failed build"))
  }
  unlink(directory,recursive=TRUE)
}
cat("metadata inspection complete zero-read reports and retained save gates passed\n")
cat("native file metadata complete zero-read reports and retained save gates passed\n")
for(kind in c("error","interrupt")) {
  failure <- structure(list(message="original inspection condition",call=NULL,payload=new.env()),class=c("inspection_test_condition",kind,"condition"))
  for(returned in c(FALSE,TRUE)) {
    if(returned && kind=="error") next
    requests <- character()
    inspect <- function(name,path) {
      requests <<- c(requests,name)
      if(name=="DM") return(list("missing",simpleError("earlier known cause")))
      if(returned) return(list("missing",failure))
      stop(failure)
    }
    for(operation in list(function() build(handle,no_port,"schema-lookup",inspect=inspect),function() specification_failure_report(handle,no_port,"schema-lookup",inspect=inspect),function() specification_report(handle,no_port,no_port,"schema-lookup",inspect=inspect))) {
      requests <- character()
      actual <- tryCatch(operation(),error=identity,interrupt=identity)
      stopifnot(identical(actual,failure),identical(requests,c("DM","AE")))
    }
  }
}
for(inspect in list(FALSE,new.env(),"missing")) {
  actual <- tryCatch(build(handle,no_port,"schema-lookup",inspect=inspect),error=identity)
  stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),"inspect must be a function"))
}
for(reply in list(TRUE,list(raw(0),FALSE),list("unknown",simpleError("opaque")),list("missing",new.env()),list("missing"))) {
  calls <- 0L
  inspect <- function(...) {calls <<- calls+1L;reply}
  actual <- tryCatch(build(handle,no_port,"schema-lookup",inspect=inspect),error=identity)
  stopifnot(inherits(actual,"error"),calls==1L)
}
cat("inspection original error and interrupt identity and closed reply validation passed\n")
# Success repeats all metadata checks before any cached source is captured.
events <- character(); seen <- character()
inspect <- function(name,path) {events <<- c(events,paste0("inspect:",name));NULL}
capture <- function(name,path,maximum) {
  events <<- c(events,paste0("capture:",name))
  content <- rawfile(file.path(case,path));stopifnot(length(content)<=maximum)
  created <- !(name %in% seen);seen <<- unique(c(seen,name))
  list(content,created)
}
for(j in seq_len(2L)) {
  events <- character()
  result <- build(handle,capture,"schema-lookup",inspect=inspect)
  stopifnot(identical(events,c("inspect:DM","inspect:AE","inspect:MEDDRA","capture:DM","capture:AE","capture:MEDDRA")),!is.null(build_output(result)))
}
cat("successful inspection repeats before cached captures passed\n")

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
# The selected lookup fixture succeeds until the host publication boundary.
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

directory <- tempfile("native-parquet-published-");dir.create(directory)
publisher <- get(".file_publisher_port",envir=asNamespace("yamaa"))(file.path(directory,"adlb.parquet"),"adlb.parquet")
for(j in seq_len(2L)) {
  actual <- publisher$save(parquet_result)
  stopifnot(identical(rawfile(file.path(directory,"adlb.parquet")),parquet_saved[[1L]]),
    identical(sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',actual),parquet_unsaved))
}
stopifnot(identical(parquet_requests,"input/lb.csv"))
unlink(directory,recursive=TRUE)
cat("native file Parquet publication retained complete report and exact held bytes passed\n")

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

# Native filesystem bytes also enter the shared held-Parquet decoder.
directory <- tempfile("native-parquet-source-"); dir.create(directory)
dir.create(file.path(directory,"input"))
stopifnot(file.copy(file.path(root,"pq","ordered-sum.parquet"),file.path(directory,"input","lb.parquet")))
handle <- prepare_entry("spec.yaml",charToRaw(source),no_port,no_port,no_port)
ports <- get(".file_resources_ports",envir=asNamespace("yamaa"))(directory,directory)
native_expected <- expected
for(created in c(1L,0L)) {
  if(created==0L) native_expected <- gsub('"snapshots_created":1','"snapshots_created":0',native_expected,fixed=TRUE)
  actual <- specification_report(handle,ports$capture,function(path,content) {
    stopifnot(path=="adlb.csv",identical(content,rawfile(file.path(case,"expected","adlb.csv"))))
    TRUE
  },case_name,inspect=ports$inspect)
  stopifnot(identical(actual,native_expected))
}
stopifnot(ports$reads()==1L)
unlink(directory,recursive=TRUE)
cat("native file Parquet source complete original report, cached bytes and exact CSV passed\n")

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
  directory <- tempfile("native-parquet-failure-"); dir.create(directory)
  stopifnot(file.copy(file.path(root,"pq",paste0(test[[1L]],".parquet")),file.path(directory,"input.parquet")))
  ports <- get(".file_resources_ports",envir=asNamespace("yamaa"))(directory,directory)
  native_expected <- expected
  for(created in c(1L,0L)) {
    if(created==0L) native_expected <- gsub('"snapshots_created":1','"snapshots_created":0',native_expected,fixed=TRUE)
    native_result <- build(handle,ports$capture,test[[1L]],inspect=ports$inspect)
    stopifnot(identical(build_observations(native_result),native_expected),is.null(build_output(native_result)))
    failure <- tryCatch(build_save(native_result,no_port),error=identity)
    stopifnot(inherits(failure,"error"),conditionMessage(failure)=="cannot save a failed build")
  }
  stopifnot(ports$reads()==1L)
  unlink(directory,recursive=TRUE)
}
cat("native Parquet source complete failed reports and save gates passed\n")
cat("native file Parquet source complete failed reports, cached bytes and save gates passed\n")

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
failed_report_truth("predicate-diagnostics.tsv","predicate",1L,charToRaw("ID,V\n1,2\n"))
cat("core predicate complete independent failed report and retained save gates passed\n")
failed_report_truth("source-typing.tsv","typing",4L)
cat("core source typing complete independent failed reports and retained save gates passed\n")

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

# Source-filter truth uses bytes rather than a host YAML/predicate planner.
hex_raw <- function(hex) {
  if(!nzchar(hex)) return(raw(0))
  starts <- seq.int(1L,nchar(hex),by=2L)
  as.raw(strtoi(substring(hex,starts,starts+1L),base=16L))
}
source_selection_truth <- function(filename,prefix,cases) {
filter_truth <- read.delim(file.path(root,filename),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="UTF-8",check.names=FALSE)
stopifnot(nrow(filter_truth)==cases)
for(i in seq_len(nrow(filter_truth))) {
  row <- filter_truth[i,,drop=FALSE]
  handle <- prepare_entry("spec.yaml",hex_raw(row$source_hex[[1L]]),no_port,no_port,no_port)
  content <- hex_raw(row$input_hex[[1L]])
  artifact <- hex_raw(row$artifact_hex[[1L]])
  # Preserve historical captures; REQ-0068 owns these row-filter failures.
  captured <- row$expected[[1L]]
  if(prefix=="row-filter" && row$case[[1L]] %in% c("grouped_source_phase","grouped_unknown_phase","record_unknown_phase","group_and_filter_order")) {
    stopifnot(length(gregexpr('"requirement":null',captured,fixed=TRUE)[[1L]])==2L)
    captured <- gsub('"requirement":null','"requirement":"REQ-0068"',captured,fixed=TRUE)
  }
  expected <- sub('"runtime":"python"','"runtime":"r"',captured,fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  unsaved <- if(length(artifact)) sub('"artifacts":\\[.*?\\],"backend"','"artifacts":[],"backend"',expected,perl=TRUE) else expected
  calls <- 0L
  capture <- function(name,path,maximum) {
    stopifnot(name=="SRC",path=="source.csv",length(content)<=maximum)
    calls <<- calls+1L;list(content,calls==1L)
  }
  saves <- 0L
  publish <- function(path,bytes) {
    stopifnot(length(artifact)>0L,path=="result.csv",identical(bytes,artifact))
    saves <<- saves+1L;TRUE
  }
  for(created in c(1L,0L)) {
    if(created==0L) {
      expected <- gsub('"snapshots_created":1','"snapshots_created":0',expected,fixed=TRUE)
      unsaved <- gsub('"snapshots_created":1','"snapshots_created":0',unsaved,fixed=TRUE)
    }
    result <- build(handle,capture,paste0(prefix,"-",row$case[[1L]]))
    stopifnot(identical(build_observations(result),unsaved))
    if(length(artifact)) {
      stopifnot(!is.null(build_output(result)))
      for(j in seq_len(2L)) stopifnot(identical(build_save(result,publish),expected))
    } else {
      stopifnot(is.null(build_output(result)))
      actual <- tryCatch(build_save(result,publish),error=identity)
      stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),"cannot save a failed build"))
    }
  }
  stopifnot(calls==2L,saves==if(length(artifact)) 4L else 0L)
}
}
# Original scalar source bindings have independently authored complete truth.
source_binding_truth <- read.delim(file.path(root,"original-source-bindings.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(source_binding_truth)==16L)
for(i in seq_len(nrow(source_binding_truth))) {
  row <- source_binding_truth[i,,drop=FALSE]
  handle <- prepare_entry("spec.yaml",hex_raw(row$source_hex[[1L]]),no_port,no_port,no_port)
  artifact <- hex_raw(row$artifact_hex[[1L]])
  expected <- sub('"runtime":"python"','"runtime":"r"',row$expected[[1L]],fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  unsaved <- if(length(artifact)) sub('"artifacts":\\[.*?\\],"backend"','"artifacts":[],"backend"',expected,perl=TRUE) else expected
  case <- file.path(root,"source-binding-cases",row$case[[1L]])
  for(j in seq_len(2L)) {
    calls <- character();saves <- 0L
    capture <- function(name,path,maximum) {
      stopifnot(path==paste0(tolower(name),".csv"))
      content <- rawfile(file.path(case,path));stopifnot(length(content)<=maximum)
      calls <<- c(calls,name);list(content,TRUE)
    }
    publish <- function(path,content) {
      stopifnot(length(artifact)>0L,path=="result.csv",identical(content,artifact))
      saves <<- saves+1L;TRUE
    }
    result <- build(handle,capture,paste0("source-binding-",row$case[[1L]]))
    stopifnot(identical(build_observations(result),unsaved),identical(calls,if(file.exists(file.path(case,"donor.csv"))) c("SRC","DONOR") else "SRC"))
    if(length(artifact)) {
      for(k in seq_len(2L)) stopifnot(identical(build_save(result,publish),expected))
      stopifnot(saves==2L)
    } else {
      stopifnot(is.null(build_output(result)))
      failure <- tryCatch(build_save(result,publish),error=identity)
      stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"),saves==0L)
    }
  }
}
cat("original source bindings complete reports and exact repeated CSV passed\n")

mapping_cut_admission <- read.delim(file.path(root,"mapping-cut-admission.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(mapping_cut_admission)==5L)
for(i in seq_len(nrow(mapping_cut_admission))) {
  row <- mapping_cut_admission[i,,drop=FALSE]
  failure <- tryCatch(prepare_entry("spec.yaml",hex_raw(row$source_hex[[1L]]),no_port,no_port,no_port),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),row$expected[[1L]]))
}
cat("original mapping/cut admission complete findings before ports passed\n")

# Original mapping and cut operations have independently authored complete truth.
mapping_cut_truth <- read.delim(file.path(root,"original-mapping-cut.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(mapping_cut_truth)==21L)
for(i in seq_len(nrow(mapping_cut_truth))) {
  row <- mapping_cut_truth[i,,drop=FALSE]
  handle <- prepare_entry("spec.yaml",hex_raw(row$source_hex[[1L]]),no_port,no_port,no_port)
  artifact <- hex_raw(row$artifact_hex[[1L]])
  expected <- sub('"runtime":"python"','"runtime":"r"',rawToChar(hex_raw(row$expected_wire_hex[[1L]])),fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  unsaved <- if(length(artifact)) sub('"artifacts":\\[.*?\\],"backend"','"artifacts":[],"backend"',expected,perl=TRUE) else expected
  case <- file.path(root,"mapping-cut-cases",paste0(row$case[[1L]],"-fixture"))
  for(j in seq_len(2L)) {
    calls <- character();saves <- 0L
    capture <- function(name,path,maximum) {
      stopifnot(path==paste0(tolower(name),".csv"))
      content <- rawfile(file.path(case,path));stopifnot(length(content)<=maximum)
      calls <<- c(calls,name);list(content,TRUE)
    }
    publish <- function(path,content) {
      stopifnot(length(artifact)>0L,path=="result.csv",identical(content,artifact))
      saves <<- saves+1L;TRUE
    }
    result <- build(handle,capture,paste0("mapping-cut-",row$case[[1L]]))
    stopifnot(identical(build_observations(result),unsaved),identical(calls,if(file.exists(file.path(case,"donor.csv"))) c("SRC","DONOR") else "SRC"))
    if(length(artifact)) {
      for(k in seq_len(2L)) stopifnot(identical(build_save(result,publish),expected))
      stopifnot(saves==2L)
    } else {
      stopifnot(is.null(build_output(result)))
      failure <- tryCatch(build_save(result,publish),error=identity)
      stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),"cannot save a failed build"),saves==0L)
    }
  }
}
cat("original mapping/cut complete reports and exact repeated CSV passed\n")

# File-backed dictionaries reuse authored inline complete report and CSV truth.
dictionaries <- c(exact_typed="{Y: 1, y: 2.0, Z: null}",
  ascii_fold=r"({'yes': 'matched', "\u00e9a": 'foreign', "\u00c9a": 'capital', "\u00df": 'sharp', 'ss': 'double'})",
  lazy_case="{Y: 1}",nested_concat="{Y: 'yes'}",record_mapping="{Y: 1}",multiple_distinct_input="{Y: yes}")
dictionary_prepare <- get(".prepare_file_specification",envir=asNamespace("yamaa"))
dictionary_build <- get(".file_specification_build",envir=asNamespace("yamaa"))
dictionary_reads <- get(".file_specification_reads",envir=asNamespace("yamaa"))
for(name in names(dictionaries)) {
  row <- mapping_cut_truth[mapping_cut_truth$case==name,,drop=FALSE]
  stopifnot(nrow(row)==1L)
  directory <- tempfile("dictionary-resource-");dir.create(directory)
  directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
  inline <- paste0("dict: ",dictionaries[[name]])
  authored <- rawToChar(hex_raw(row$source_hex[[1L]]))
  stopifnot(grepl(inline,authored,fixed=TRUE))
  writeBin(charToRaw(sub(inline,"dict: ./dictionary.yaml",authored,fixed=TRUE)),file.path(directory,"spec.yaml"))
  writeBin(charToRaw(paste0(dictionaries[[name]],"\n")),file.path(directory,"dictionary.yaml"))
  case <- file.path(root,"mapping-cut-cases",paste0(name,"-fixture"))
  stopifnot(file.copy(file.path(case,"src.csv"),file.path(directory,"src.csv")))
  handle <- dictionary_prepare(directory,directory,"spec.yaml")
  stopifnot(dictionary_reads(handle)==2L)
  expected <- sub('"runtime":"python"','"runtime":"r"',rawToChar(hex_raw(row$expected_wire_hex[[1L]])),fixed=TRUE)
  expected <- sub('fixture-runtime',as.character(getRversion()),expected,fixed=TRUE)
  expected <- sub('fixture-engine',engine_info()$core_version,expected,fixed=TRUE)
  artifact <- hex_raw(row$artifact_hex[[1L]])
  for(created in c(1L,0L)) {
    if(created==0L) expected <- sub('"snapshots_created":1','"snapshots_created":0',expected,fixed=TRUE)
    unsaved <- if(length(artifact)) sub('"artifacts":\\[.*?\\],"backend"','"artifacts":[],"backend"',expected,perl=TRUE) else expected
    result <- dictionary_build(handle,paste0("mapping-cut-",name))
    stopifnot(identical(build_observations(result),unsaved),dictionary_reads(handle)==3L)
    saves <- 0L
    publish <- function(path,content) {
      stopifnot(path=="result.csv",identical(content,artifact))
      writeBin(content,file.path(directory,path));saves <<- saves+1L;TRUE
    }
    if(length(artifact)) {
      stopifnot(identical(build_save(result,publish),expected),saves==1L,identical(rawfile(file.path(directory,"result.csv")),artifact))
    } else {
      failure <- tryCatch(build_save(result,publish),error=identity)
      stopifnot(is.null(build_output(result)),inherits(failure,"error"),saves==0L,!file.exists(file.path(directory,"result.csv")))
    }
  }
  unlink(directory,recursive=TRUE)
}
cat("original dictionary resources complete reports and exact repeated CSV passed\n")

source_selection_truth("source-filters.tsv","source-filter",13L)
cat("original source filters complete reports, cached reads and exact saved CSV passed\n")
source_selection_truth("first-available.tsv","first-available",11L)
cat("original first available complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("original-assertions.tsv","assert",17L)
cat("original assertions complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("original-all-or-none.tsv","all-or-none",11L)
cat("original all or none complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("original-not-missing.tsv","column-check",10L)
cat("original column not missing complete reports, cached reads and exact saved CSV passed\n")

row_filter_admission <- read.delim(file.path(root,"row-filter-admission.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
stopifnot(nrow(row_filter_admission)==2L)
for(i in seq_len(nrow(row_filter_admission))) {
  hex <- row_filter_admission$source_hex[[i]]
  starts <- seq.int(1L,nchar(hex),by=2L)
  bytes <- as.raw(strtoi(substring(hex,starts,starts+1L),base=16L))
  failure <- tryCatch(prepare_entry("spec.yaml",bytes,no_port,no_port,no_port),error=identity)
  stopifnot(inherits(failure,"error"),identical(conditionMessage(failure),row_filter_admission$expected[[i]]))
}
cat("row-filter unsupported admission before all ports passed\n")
source_selection_truth("row-filters.tsv","row-filter",27L)
cat("row-template filters complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("row-reductions.tsv","row-reduction",25L)
cat("row-template MEAN and COUNT complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("row-column-checks.tsv","row-column",48L)
cat("row-template column checks complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("original-column-values.tsv","column-value",20L)
cat("original column values complete reports, cached reads and exact saved CSV passed\n")

source_selection_truth("original-column-matches.tsv","column-matches",16L)
cat("original column matches complete reports, cached reads and exact saved CSV passed\n")

static_truth <- read.delim(file.path(root,"static-verification-checks.tsv"),sep="\t",quote="",comment.char="",colClasses="character",fileEncoding="UTF-8",check.names=FALSE)
stopifnot(nrow(static_truth)==15L)
check_issues <- get(".specification_check_issues",envir=asNamespace("yamaa"))
for(i in seq_len(nrow(static_truth))) {
  handle <- prepare_entry("spec.yaml",hex_raw(static_truth$source_hex[[i]]),no_port,no_port,no_port)
  for(j in seq_len(2L)) stopifnot(identical(check_issues(handle),static_truth$expected[[i]]))
  frame <- get(".check_specification_issue_frame",envir=asNamespace("yamaa"))
  for(j in seq_len(2L)) check_issue_frame(frame(handle),issue_frame_truth[[paste0("static/",static_truth$case[[i]])]])
}
cat("static verification check complete issues without study authority passed\n")
cat("static check issue data frames match fifteen independent payloads without study authority passed\n")

# File preparation retains complete early diagnostics and cross-directory authority.
directory <- tempfile("native-file-preparation-");dir.create(directory)
directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
dir.create(file.path(directory,"entry"));dir.create(file.path(directory,"parent"))
entry_path <- file.path(directory,"entry","spec.yaml")
valid <- "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n"
prepare_file <- get(".prepare_file_specification",envir=asNamespace("yamaa"))
file_check <- get(".file_specification_check",envir=asNamespace("yamaa"))
file_reads <- get(".file_specification_reads",envir=asNamespace("yamaa"))
file_build <- get(".file_specification_build",envir=asNamespace("yamaa"))
writeBin(charToRaw(paste0(sub("'1.0'","'99.0'",valid,fixed=TRUE),"parents: absent.yaml\n")),entry_path)
actual <- tryCatch(prepare_file(directory,file.path(directory,"entry"),"spec.yaml"),error=identity)
expected <- paste0('{"outcome":{"diagnostics":[{"condition":"schema_version_mismatch","context":{"actual":"99.0","entry":"',entry_path,'","expected":"1.0","source":"',entry_path,'"},"phase":"validation","requirement":"REQ-0245","spec_paths":["schema_version"]}],"status":"invalid"},"protocol":"specification/prototype"}')
stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),expected))
writeBin(charToRaw(paste0(valid,"parents: absent.yaml\n")),entry_path)
actual <- tryCatch(prepare_file(directory,file.path(directory,"entry"),"spec.yaml"),error=identity)
expected <- paste0('{"outcome":{"diagnostics":[{"condition":"parent_not_found","context":{"path":"absent.yaml","source":"',entry_path,'"},"phase":"validation","requirement":"REQ-0654","spec_paths":["parents"]}],"status":"invalid"},"protocol":"specification/prototype"}')
stopifnot(inherits(actual,"error"),identical(conditionMessage(actual),expected))
writeBin(charToRaw(valid),file.path(directory,"parent","base.yaml"))
writeBin(charToRaw("schema_version: '1.0'\nparents: [../parent/base.yaml, ../parent/./base.yaml]\n"),entry_path)
handle <- prepare_file(directory,file.path(directory,"entry"),"spec.yaml")
stopifnot(file_reads(handle)==2L,identical(file_check(handle),"[]"),file_reads(handle)==2L)
stopifnot(identical(get(".file_specification_source",envir=asNamespace("yamaa"))(handle),list(name="SRC",path="../parent/input.csv")))
writeBin(charToRaw("[changed"),entry_path);writeBin(charToRaw("[changed"),file.path(directory,"parent","base.yaml"))
writeBin(charToRaw("ID\n1\n"),file.path(directory,"parent","input.csv"))
result <- file_build(handle,"file-view")
held_output <- build_output(result)
stopifnot(identical(table_snapshot(held_output),'{"protocol":"table/1","columns":[["ID","int"]],"row_count":"1","chunks":["1"],"rows":[[{"int":"1"}]]}'),file_reads(handle)==3L)
writeBin(charToRaw("ID\n2\n"),file.path(directory,"parent","input.csv"))
actual <- tryCatch(file_build(handle,"file-view"),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="captured resource content changed",identical(build_output(result),held_output),file_reads(handle)==3L)
for(roots in list(rep(directory,64L),NA_character_,1L,structure(directory,class="roots")))
  stopifnot(inherits(tryCatch(prepare_file(directory,directory,"spec.yaml",roots),error=identity),"error"))
stopifnot(inherits(tryCatch(file_check(result),error=identity),"error"))
unlink(directory,recursive=TRUE)
cat("native file preparation complete early findings, lexical views, retained model and native resource failures passed\n")

# Byte transport keeps bounded captures, authority and registered handle lifetimes.
directory <- tempfile("native-resource-policy-"); dir.create(directory)
dir.create(file.path(directory,"spec")); dir.create(file.path(directory,"data"))
writeBin(charToRaw("retained"),file.path(directory,"data","source"))
file_ports <- get(".file_resources_ports",envir=asNamespace("yamaa"))
ports <- file_ports(directory,file.path(directory,"spec"))
stopifnot(is.null(ports$inspect("SRC","../data/source")),ports$reads()==0L)
actual <- tryCatch(ports$capture("SRC","../data/source",7L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="resource capture limit",ports$reads()==0L)
first <- ports$capture("SRC","../data/source",8L)
stopifnot(identical(first,list(charToRaw("retained"),TRUE)),ports$reads()==1L)
first[[1L]][[1L]] <- as.raw(0L)
stopifnot(identical(ports$capture("SRC","../data/source",8L),list(charToRaw("retained"),FALSE)),ports$reads()==1L)
# Both spellings of one canonical key remain authority witnesses.
stopifnot(identical(ports$capture("SRC","data/source",8L),list(charToRaw("retained"),FALSE)))
dir.create(file.path(directory,"spec","data"))
writeBin(charToRaw("shadowed"),file.path(directory,"spec","data","source"))
actual <- tryCatch(ports$capture("SRC","../data/source",8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="captured resource content changed",ports$reads()==1L)
unlink(file.path(directory,"spec","data"),recursive=TRUE)
stopifnot(identical(ports$capture("SRC","../data/source",8L),list(charToRaw("retained"),FALSE)))
for(maximum in list(-1L,NA_integer_,Inf,1.5,TRUE,"8",structure(8L,class="ceiling"),1e100,1+0i)) {
  stopifnot(inherits(tryCatch(ports$capture("SRC","../data/source",maximum),error=identity),"error"))
}
writeBin(charToRaw("changed!"),file.path(directory,"data","source"))
actual <- tryCatch(ports$capture("SRC","../data/source",8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="captured resource content changed",ports$reads()==1L)
fresh <- file_ports(directory,file.path(directory,"spec"))
stopifnot(identical(fresh$capture("SRC","../data/source",8L),list(charToRaw("changed!"),TRUE)))
stopifnot(identical(ports$inspect("SRC","absent")[[1L]],"missing"))
stopifnot(identical(ports$inspect("SRC","../data")[[1L]],"not_regular_file"))
outside <- tempfile("outside-native-root-"); writeBin(charToRaw("outside"),outside)
stopifnot(file.symlink(outside,file.path(directory,"spec","link")))
actual <- tryCatch(ports$capture("SRC","link",8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="resource path contains a symbolic link")
actual <- tryCatch(ports$capture("SRC",normalizePath(outside,winslash="/",mustWork=TRUE),8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="resource path outside approved roots")
expired <- unserialize(serialize(ports$handle,NULL))
reply <- .Call(get("wrap__file_resource_reads",envir=asNamespace("yamaa")),expired)
stopifnot(!is.null(reply$error))
gc()
stopifnot(ports$reads()==1L)
# A stale physical cache entry never poisons a newly authored path; old paths
# keep their original immutable snapshots and fail after a content change.
writeBin(charToRaw("original"),file.path(directory,"spec","stale-a"))
stopifnot(file.link(file.path(directory,"spec","stale-a"),file.path(directory,"spec","new-c")))
stale <- file_ports(directory,file.path(directory,"spec"))
first <- stale$capture("SRC","stale-a",8L)
unlink(file.path(directory,"spec","stale-a"))
stopifnot(identical(stale$capture("SRC","new-c",8L),list(charToRaw("original"),TRUE)),stale$reads()==2L)
stopifnot(identical(stale$capture("SRC","new-c",8L),list(charToRaw("original"),FALSE)),identical(first,list(charToRaw("original"),TRUE)))
writeBin(charToRaw("original"),file.path(directory,"spec","changed-a"))
stopifnot(file.link(file.path(directory,"spec","changed-a"),file.path(directory,"spec","changed-c")))
changed <- file_ports(directory,file.path(directory,"spec"))
first <- changed$capture("SRC","changed-a",8L)
writeBin(charToRaw("changed!"),file.path(directory,"spec","changed-a"))
actual <- tryCatch(changed$capture("SRC","changed-a",8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="captured resource content changed")
stopifnot(identical(changed$capture("SRC","changed-c",8L),list(charToRaw("changed!"),TRUE)),changed$reads()==2L)
actual <- tryCatch(changed$capture("SRC","changed-a",8L),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="captured resource content changed",identical(first,list(charToRaw("original"),TRUE)))
unlink(directory,recursive=TRUE); unlink(outside)
cat("native file byte ceilings, snapshot mutation, link authority and registered handles passed\n")

# Explicit publication authority is separate from read roots.
directory <- tempfile("native-publication-policy-");dir.create(directory)
target <- file.path(directory,"target.csv")
writeBin(charToRaw("retained"),target)
file_publisher <- get(".file_publisher_port",envir=asNamespace("yamaa"))
for(name in c("new.csv","target.csv")) for(suffix in c("/","/.","/..")) {
  actual <- tryCatch(file_publisher(paste0(file.path(directory,name),suffix),"declared.csv"),error=identity)
  stopifnot(inherits(actual,"error"),conditionMessage(actual)=="invalid explicit publication target",identical(rawfile(target),charToRaw("retained")),identical(list.files(directory,all.files=TRUE,no..=TRUE),"target.csv"))
}
publication <- file_publisher(target,"declared.csv")
actual <- tryCatch(publication$publish("other.csv",charToRaw("new")),error=identity)
stopifnot(inherits(actual,"error"),conditionMessage(actual)=="publication path does not match explicit target",identical(rawfile(target),charToRaw("retained")))
for(content in list("new",1L,structure(charToRaw("new"),class="content"))) {
  stopifnot(inherits(tryCatch(publication$publish("declared.csv",content),error=identity),"error"))
}
stopifnot(identical(publication$publish("declared.csv",charToRaw("complete")),TRUE),identical(rawfile(target),charToRaw("complete")))
expired <- unserialize(serialize(publication$handle,NULL))
reply <- .Call(get("wrap__publish_file_artifact",envir=asNamespace("yamaa")),expired,charToRaw("declared.csv"),charToRaw("must not write"))
stopifnot(!is.null(reply$error),identical(rawfile(target),charToRaw("complete")))
stopifnot(identical(list.files(directory,all.files=TRUE,no..=TRUE),"target.csv"))
unlink(directory,recursive=TRUE)
cat("native file publication explicit authority, raw transport and registered handles passed\n")

source(file.path(root,"public-output-truth.R"),local=TRUE)
domain_observations <- get(".domain_observations",envir=asNamespace("yamaa"))
json_text <- function(value) {
  # Quote only run-identity labels; original semantic truth remains unchanged.
  characters <- strsplit(enc2utf8(value),"",fixed=TRUE)[[1L]]
  escaped <- vapply(characters,function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code),
      "8"="\\b","9"="\\t","10"="\\n","12"="\\f","13"="\\r",
      "34"="\\\"","92"="\\\\",
      if(code<32L) sprintf("\\u%04x",code) else character)
  },character(1L),USE.NAMES=FALSE)
  paste0('"',paste0(escaped,collapse=""),'"')
}
for(case_name in names(issue_frame_truth)[!startsWith(names(issue_frame_truth),"static/")]) {
  directory <- tempfile("public-domain-");dir.create(directory)
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,if(case_name=="schema-inheritance") "spec_study.yaml" else "spec.yaml")
  built <- yamaa_domain(entry)
  report <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
  report <- sub('"runtime":"python"','"runtime":"r"',report,fixed=TRUE)
  report <- sub('fixture-runtime',as.character(getRversion()),report,fixed=TRUE)
  report <- sub('fixture-engine',engine_info()$core_version,report,fixed=TRUE)
  report <- sub(paste0('"example":"',case_name,'"'),'"example":"domain"',report,fixed=TRUE)
  entry_field <- paste0('"specification":',json_text(entry))
  report <- gsub(paste0('"specification":',json_text(basename(entry))),
                 entry_field,report,fixed=TRUE)
  unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',report)
  observed <- rawToChar(domain_observations(built))
  if(!identical(observed,unsaved)) {
    cat(case_name,"public unsaved report mismatch\nexpected:\n",unsaved,
        "\nactual:\n",observed,"\n",sep="")
  }
  stopifnot(identical(observed,unsaved))
  check_issue_frame(built$issues,issue_frame_truth[[case_name]])
  stopifnot(is.null(built$verification_log),is.null(built$warning_log))
  expected <- public_output_truth[[case_name]]
  if(is.null(expected)) {
    stopifnot(is.null(built$output),inherits(tryCatch(built$save(),error=identity),"yamaa_domain_error"))
    stopifnot(identical(rawToChar(domain_observations(built)),unsaved))
  } else {
    stopifnot(is.data.frame(built$output),identical(as.list(built$output),expected))
    output_name <- switch(case_name,"schema-lookup"="adsl.csv","schema-window-functions"="advs.csv","adlb.csv")
    for(attempt in seq_len(2L)) {
      stopifnot(identical(built$save(),TRUE),identical(rawfile(file.path(case,output_name)),rawfile(file.path(case,"expected",output_name))))
      stopifnot(identical(rawToChar(domain_observations(built)),report))
    }
    stopifnot(identical(as.list(built$output),expected))
  }
  report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  if(nzchar(report_directory)) {
    # Relabel only the observed run identity; semantic findings and reads stay exact.
    observed <- rawToChar(domain_observations(built))
    observed <- sub('"example":"domain"',paste0('"example":"',case_name,'"'),observed,fixed=TRUE)
    observed <- gsub(entry_field,
                     paste0('"specification":',json_text(basename(entry))),observed,fixed=TRUE)
    dir.create(report_directory,recursive=TRUE,showWarnings=FALSE)
    writeBin(charToRaw(observed),file.path(report_directory,paste0(case_name,".r.rust.json")))
  }
  unlink(directory,recursive=TRUE)
  cat(case_name,"public domain typed output, complete issues and exact saved bytes passed\n")
  cat(case_name,"public domain complete Rust run report and successful save observations passed\n")
}
directory <- tempfile("public-check-");dir.create(directory)
entry <- file.path(directory,"spec.yaml")
writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
             "output: {path: absent/output.csv, columns: [ID]}","columns:",
             "  - {name: ID, type: int, derivation: {source: SRC.ID}}"),entry)
stopifnot(nrow(yamaa_check(entry)$issues)==0L)
failed <- yamaa_domain(entry)
stopifnot(is.null(failed$output),identical(failed$issues$condition,"resource_path_missing"),inherits(tryCatch(failed$save(),error=identity),"yamaa_domain_error"))
writeBin(charToRaw("ID\n-9223372036854775808\n9223372036854775807\n"),file.path(directory,"input.csv"))
built <- yamaa_domain(entry)
stopifnot(identical(as.character(built$output$ID),c("-9223372036854775808","9223372036854775807")),identical(is.na(built$output$ID),c(FALSE,FALSE)))
writeLines("[changed",entry); writeLines("changed",file.path(directory,"input.csv"))
stopifnot(identical(built$save(),FALSE),identical(built$issues$condition,"engine_rejected"))
dir.create(file.path(directory,"absent"))
stopifnot(identical(built$save(),TRUE),nrow(built$issues)==0L,identical(rawfile(file.path(directory,"absent/output.csv")),charToRaw("ID\n-9223372036854775808\n9223372036854775807\n")))
refused <- yamaa_domain("absent.yaml",environment="absent-environment.yaml")
stopifnot(is.null(domain_observations(refused)))
stopifnot(is.null(refused$output),identical(refused$issues$condition,"engine_rejected"),identical(refused$issues,yamaa_check("absent.yaml",environment="absent-environment.yaml")$issues))
stopifnot(inherits(tryCatch(yamaa_domain(NULL),error=identity),"error"))
for(path in c(strrep("x",65537L),strrep("\u00e9",32769L))) {
  refused <- yamaa_domain(path)
  stopifnot(is.null(refused$output),identical(refused$issues$condition,"engine_rejected"),
            identical(refused$issues$context,'{"code":"resource_path","stage":"prepare"}'),
            identical(yamaa_check(path)$issues,refused$issues),
            identical(yamaa_domain(path,environment=path)$issues$condition,"engine_rejected"))
}
unlink(directory,recursive=TRUE)
cat("public check without study data, full-range integers, retained save retry and environment refusal passed\n")

remove_directory_link <- function(path) {
  if(.Platform$OS.type=="windows") {
    # R's unlink treats directory symbolic links as mount-point reparse tags.
    # rmdir without /s removes only this link, preserving its external target.
    command <- Sys.getenv("COMSPEC")
    stopifnot(nzchar(command),file.exists(command))
    system2(command,c("/d","/c","rmdir",shQuote(chartr("/","\\",path),type="cmd")))
  } else unlink(path,recursive=TRUE)
}
for(late in c(FALSE,TRUE)) for(existing in c(FALSE,TRUE)) {
  directory <- tempfile("public-linked-output-");dir.create(directory)
  outside <- tempfile("public-output-outside-");dir.create(outside)
  sentinel <- file.path(outside,"sentinel")
  sentinel_bytes <- charToRaw("outside sentinel")
  writeBin(sentinel_bytes,sentinel)
  entry <- file.path(directory,"spec.yaml")
  writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]","input: {SRC: input.csv}",
               "output: {path: link/output.csv, columns: [ID]}","columns:",
               "  - {name: ID, type: int, derivation: {source: SRC.ID}}"),entry)
  writeBin(charToRaw("ID\n1\n"),file.path(directory,"input.csv"))
  target <- file.path(outside,"output.csv")
  if(existing) writeBin(charToRaw("retained"),target)
  link <- file.path(directory,"link")
  if(late) dir.create(link) else stopifnot(file.symlink(outside,link))
  built <- yamaa_domain(entry)
  stopifnot(identical(as.character(built$output$ID),"1"))
  if(late) {
    stopifnot(identical(unlink(link,recursive=TRUE),0L),!file.exists(link),file.symlink(outside,link))
  }
  stopifnot(identical(built$save(),FALSE),identical(built$issues$context,'{"code":"invalid_target","stage":"output"}'),
            identical(list.files(outside,all.files=TRUE,no..=TRUE),if(existing) c("output.csv","sentinel") else "sentinel"))
  stopifnot(identical(rawfile(sentinel),sentinel_bytes))
  if(existing) stopifnot(identical(rawfile(target),charToRaw("retained")))
  stopifnot(identical(remove_directory_link(link),0L),!file.exists(link),
            identical(rawfile(sentinel),sentinel_bytes))
  if(existing) stopifnot(identical(rawfile(target),charToRaw("retained")))
  stopifnot(dir.create(link))
  stopifnot(identical(built$save(),TRUE),nrow(built$issues)==0L,identical(rawfile(file.path(link,"output.csv")),charToRaw("ID\n1\n")))
  stopifnot(identical(rawfile(sentinel),sentinel_bytes))
  unlink(directory,recursive=TRUE);unlink(outside,recursive=TRUE)
}
cat("public save rejects linked parents before or after build, retains external targets and permits safe retry passed\n")


# Actual installed R capabilities run the owned project service before study IO.
local({
  ns <- asNamespace("yamaa")
  prepare <- get(".prepare_file_project",ns)
  status <- get(".file_project_status",ns)
  reads <- get(".file_project_reads",ns)
  build <- get(".file_project_build",ns)
  observations <- get(".file_project_observations",ns)
  result <- get(".file_project_result",ns)
  result_status <- get(".project_result_status",ns)
  result_output <- get(".project_result_output",ns)
  result_issues <- get(".project_result_issues",ns)
  retained <- get(".project_result_retained",ns)
  result_report <- get(".project_result_observations",ns)
  result_save <- get(".project_result_save",ns)
  propagate <- get(".project_result_propagate_interrupt",ns)
  names <- c("schema.yaml",sort(setdiff(list.files(file.path(root,"schema"),pattern="[.]yaml$"),"schema.yaml")))
  modules <- setNames(lapply(file.path(root,"schema",names),rawfile),names)
  envroot <- system.file("specification-environment-candidate",package="yamaa",mustWork=TRUE)
  names <- c("schema_environment.yaml","schema_shared.yaml")
  envmodules <- setNames(lapply(file.path(envroot,names),rawfile),names)
  directory <- tempfile("owned-project-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  lock <- charToRaw(paste0('{"R":{"Version":"',getRversion(),'"},"Packages":{"yamaa":{"Version":"',utils::packageVersion("yamaa"),'"}}}'))
  writeBin(lock,file.path(directory,"renv.lock"))
  writeBin(charToRaw("schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions:\n  id:\n    function: base::identity\n    description: Integer identity.\n    params: [{name: x, type: int}]\n    returns: int\n    tests:\n      - {id: normal, covers: [normal], args: {x: 7}, result: 7}\n      - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775808}\n      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"),file.path(directory,"environment.yaml"))
  writeBin(charToRaw("schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: V, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: result.csv, columns: [ID, V]}\n"),file.path(directory,"domain.yaml"))
  writeBin(charToRaw("ID\n1\n"),file.path(directory,"input.csv"))
  project <- prepare(directory,directory,"domain.yaml","environment.yaml",modules,envmodules)
  stopifnot(identical(status(project),"ready"))
  first <- build(project); first_observed <- observations(first)
  metadata <- list(as.character(getRversion()),"0.1.0","owned-project","domain.yaml",directory)
  complete <- result(first,metadata)
  stopifnot(identical(result_status(complete),"complete"),is.raw(result_output(complete)),nrow(result_issues(complete))==0L)
  published <- list()
  saved <- result_save(complete,function(path,content) { published[[length(published)+1L]] <<- list(path,content);TRUE })
  stopifnot(identical(published,list(list("result.csv",charToRaw("ID,V\n1,1\n")))),
            grepl('"outcome":"success"',saved,fixed=TRUE))
  stopifnot(identical(first_observed$engine_succeeded,TRUE),identical(first_observed$host_failures,list()))
  stopifnot(identical(first_observed$prepared_sources[[1L]]$bytes,rawfile(file.path(directory,"domain.yaml"))),
            identical(first_observed$prepared_sources[[2L]]$bytes,rawfile(file.path(directory,"environment.yaml"))))
  text <- rawToChar(first_observed$activation)
  stopifnot(grepl('"actual":{"int":"-9223372036854775808"}',text,fixed=TRUE),
            grepl('"actual":{"missing":null}',text,fixed=TRUE),grepl('"invoked":false',text,fixed=TRUE))
  before <- reads(project)
  writeBin(charToRaw('{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"999.0"}}}'),file.path(directory,"renv.lock"))
  second <- build(project)
  stopifnot(identical(observations(second)$engine_succeeded,TRUE),identical(reads(project),before),identical(observations(first),first_observed))
  writeBin(charToRaw("ID\n999\n"),file.path(directory,"input.csv"))
  changed <- observations(build(project))
  stopifnot(identical(changed$engine_succeeded,FALSE),identical(observations(first),first_observed))
  rejected <- prepare(directory,directory,"domain.yaml","environment.yaml",modules,envmodules)
  before <- reads(rejected)
  failed_attempt <- build(rejected)
  failed <- observations(failed_attempt)
  rejected_result <- result(failed_attempt,metadata)
  failed_publish <- 0L
  forbidden_publish <- function(...) { failed_publish <<- failed_publish+1L;TRUE }
  stopifnot(identical(result_status(rejected_result),"complete"),is.null(result_output(rejected_result)),nrow(result_issues(rejected_result))>0L,
            inherits(tryCatch(result_save(rejected_result,forbidden_publish),error=identity),"error"),identical(failed_publish,0L))
  stopifnot(identical(failed$engine_succeeded,FALSE),identical(reads(rejected),before),
            identical(failed$host_failures[[1L]]$stage,"lock"),
            identical(failed$host_failures[[1L]]$facts$findings[[1L]]$reason,"version_mismatch"))
  stopifnot(inherits(tryCatch(status(new.env()),error=identity),"error"),
            inherits(tryCatch(observations(project),error=identity),"error"))
  writeBin(lock,file.path(directory,"renv.lock"))
  writeBin(charToRaw("ID\n1\n"),file.path(directory,"input.csv"))
  # Exercise the actual installed closed-envelope factory with an adversarial
  # invocation boundary. Its production namespace and metadata helper stay held.
  host <- new.env(parent=ns)
  factory <- get(".yamaa_locked_host_capabilities",ns); environment(factory) <- host
  native_build <- get("wrap__file_project_build",ns)
  for (mode in c("error","interrupt","live")) {
    original <- structure(list(message=paste("original",mode),call=NULL,token=new.env()),
      class=if(mode=="interrupt") c("interrupt","condition") else c("original_error","error","condition"))
    invocations <- 0L
    host$.yamaa_resolve_locked_function <- function(call,parameters) function(x) {
      invocations <<- invocations + 1L
      if(mode!="live" || invocations>2L) stop(original)
      x
    }
    capabilities <- factory()
    checked <- prepare(directory,directory,"domain.yaml","environment.yaml",modules,envmodules)
    before <- reads(checked)
    reply <- .Call(native_build,checked,capabilities$verify,capabilities$resolve)
    stopifnot(is.null(reply$error))
    attempted <- observations(reply$value)
    held <- result(reply$value,metadata)
    rm(reply);invisible(gc())
    stopifnot(all(vapply(retained(held)$host_failures,function(failure) identical(failure$facts$condition,original),logical(1))))
    if(mode=="interrupt") {
      stopifnot(identical(result_status(held),"activation_boundary"),
                identical(tryCatch(propagate(held),interrupt=identity),original),
                inherits(tryCatch(result_report(held),error=identity),"error"))
    } else {
      issues <- result_issues(held)
      stopifnot(identical(result_status(held),"complete"),is.null(result_output(held)),nrow(issues)>0L,
                grepl('"host_error":"original_error"',issues$context[1L],fixed=TRUE),
                grepl(paste("original",mode),issues$context[1L],fixed=TRUE),
                inherits(tryCatch(result_save(held,forbidden_publish),error=identity),"error"),identical(failed_publish,0L))
    }
    stopifnot(identical(attempted$engine_succeeded,FALSE),length(attempted$host_failures)>0L,
      all(vapply(attempted$host_failures,function(failure) identical(failure$facts$condition,original),logical(1))))
    if(mode=="live") stopifnot(identical(invocations,3L),reads(checked)==before+1L,
      identical(attempted$host_failures[[1L]]$stage,"derivation"))
    else stopifnot(identical(reads(checked),before),identical(attempted$host_failures[[1L]]$stage,"conformance"))
    if(mode=="interrupt") stopifnot(identical(invocations,1L),identical(attempted$host_failures[[1L]]$facts$kind,"interrupt"))
  }
  # The complete aggregate report can exceed its quota while all actual
  # ordinary failures remain retained after the original handles are collected.
  cases <- vapply(seq_len(358L),function(index) paste0("      - {id: quota",index,", covers: [normal], args: {x: 7}, result: 7}\n"),"")
  writeBin(charToRaw(paste0(rawToChar(rawfile(file.path(directory,"environment.yaml"))),paste0(cases,collapse=""))),file.path(directory,"wide-environment.yaml"))
  large <- structure(list(message=paste(rep("x",8192L),collapse=""),call=NULL,token=new.env()),class=c("large_error","error","condition"))
  host$.yamaa_resolve_locked_function <- function(call,parameters) function(x) stop(large)
  capabilities <- factory()
  wide <- prepare(directory,directory,"domain.yaml","wide-environment.yaml",modules,envmodules)
  stopifnot(identical(status(wide),"ready"))
  before <- reads(wide)
  reply <- .Call(native_build,wide,capabilities$verify,capabilities$resolve)
  stopifnot(is.null(reply$error),identical(reads(wide),before),length(observations(reply$value)$host_failures)==360L)
  limited <- result(reply$value,metadata)
  rm(reply,wide);invisible(gc())
  stopifnot(identical(result_status(limited),"report_limit"),length(retained(limited)$host_failures)==360L,
    all(vapply(retained(limited)$host_failures,function(failure) identical(failure$facts$condition,large),logical(1))),
    inherits(tryCatch(result_save(limited,forbidden_publish),error=identity),"error"),identical(failed_publish,0L))
  writeBin(charToRaw("[changed"),file.path(directory,"domain.yaml"))
  writeBin(charToRaw("[changed"),file.path(directory,"environment.yaml"))
  rm(project); invisible(gc())
  stopifnot(identical(observations(first),first_observed))
  rm(first);invisible(gc())
  stopifnot(identical(retained(complete),first_observed),is.raw(result_output(complete)),nrow(result_issues(complete))==0L)
  publisher <- get(".file_publisher_port",ns)(file.path(directory,"retained.csv"),"result.csv")
  get(".project_result_save_file",ns)(complete,publisher$handle)
  stopifnot(identical(rawfile(file.path(directory,"retained.csv")),charToRaw("ID,V\n1,1\n")))
  cat("owned installed R project results retain complete reports, exact saves and original conditions after attempt collection passed\n")
  cat("owned installed R project runs retain fresh activation, held lock/source, i64/missing evidence, original conditions/interrupts, lock rejection and registered handle authority passed\n")
})

local({
  directory <- tempfile("public-rounding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  case_name <- "schema-round-half-away-from-zero-ties"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry)
  input <- file.path(case,"input","adsl.csv")
  captured <- rawfile(input)
  stopifnot(unlink(input)==0L,!file.exists(input))
  stopifnot(nrow(yamaa_check(entry)$issues)==0L)
  writeBin(captured,input)
  result <- yamaa_domain(entry)
  expected <- read.csv(file.path(case,"expected","adsl.csv"),colClasses=c("character","character",rep("numeric",4L)),na.strings="",check.names=FALSE)
  stopifnot(nrow(expected)==16L,nrow(result$issues)==0L,identical(result$output,expected),
            identical(rawfile(entry),written),!file.exists(file.path(case,"adsl.csv")))
  stopifnot(identical(result$save(),TRUE),identical(read.csv(file.path(case,"adsl.csv"),colClasses=c("character","character",rep("numeric",4L)),na.strings="",check.names=FALSE),expected))
  case_name <- "negative-rounding-source"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  input_directory <- file.path(case,"input")
  stopifnot(unlink(input_directory,recursive=TRUE)==0L,!file.exists(input_directory))
  expected_error <- paste0("phase: validation\ncondition: incompatible_input_type\nspec_paths:\n",
    "  - columns.RNDVAL.derivation.round_half_away_from_zero.source\n",
    "requirement: REQ-0418\ncontext:\n  source: SEX\n  expected: numeric\n  actual: str\n")
  stopifnot(identical(rawToChar(rawfile(file.path(case,"expected","error.yaml"))),expected_error))
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues
  rejected <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),
    identical(checked$condition,"incompatible_input_type"),identical(checked$requirement,"REQ-0418"),
    identical(checked$spec_paths,list("columns.RNDVAL.derivation.round_half_away_from_zero.source")),
    identical(checked$context,'{"actual":"str","expected":"numeric","source":"SEX"}'),
    identical(rejected$issues,checked),is.null(rejected$output),
    is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),
    !file.exists(file.path(case,"adsl.csv")))
  cat("public original declarative rounding matches 16 committed rows and rejects original nonnumeric input before study capture without Python passed\n")
})

local({
  directory <- tempfile("public-temporal-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("adam-adsl-duration","adam-adsl-analysis-age")) {
    stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
    stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
    result <- yamaa_domain(entry)
    expected <- read.csv(file.path(case,"expected","adsl.csv"),na.strings="",check.names=FALSE)
    dates <- if(case_name=="adam-adsl-duration") c("STDT","ENDT") else c("BRTHDT","RANDDT")
    for(column in dates) expected[[column]] <- as.Date(expected[[column]])
    integers <- if(case_name=="adam-adsl-duration") c("DURW","DURM") else "AAGE"
    for(column in integers) expected[[column]] <- structure(as.character(expected[[column]]),class="yamaa_int64_vector")
    stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
      !file.exists(file.path(case,"adsl.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"adsl.csv")),rawfile(file.path(case,"expected","adsl.csv"))))
  }
  cases <- list(
    list("negative-date-diff-endpoints","columns.SURVDD.derivation.date_diff.start","RFSTDTM","datetime"),
    list("negative-study-day-datetime","columns.DTHDY.derivation.study_day.date","DTHDTM","datetime"),
    list("negative-epoch-day-non-date","columns.EXSTDY.derivation.to_epoch_day.source","EX.EXSTDT","str"),
    list("negative-date-diff-units","columns.TRTDURM.derivation.date_diff.bounds",NULL,NULL))
  for(item in cases) {
    stopifnot(file.copy(file.path(root,"cases",item[[1L]]),directory,recursive=TRUE))
    case <- file.path(directory,item[[1L]]);unlink(file.path(case,"input"),recursive=TRUE)
    bounds <- is.null(item[[3L]])
    condition <- if(bounds) "value_not_permitted" else "incompatible_input_type"
    requirement <- if(bounds) "REQ-0613" else "REQ-0606"
    context_yaml <- if(bounds) "  value: inclusive\n  permitted: [exclusive]\n" else paste0("  source: ",item[[3L]],"\n  expected: date\n  actual: ",item[[4L]],"\n")
    context <- if(bounds) '{"permitted":["exclusive"],"value":"inclusive"}' else paste0('{"actual":"',item[[4L]],'","expected":"date","source":"',item[[3L]],'"}')
    stopifnot(identical(rawToChar(rawfile(file.path(case,"expected","error.yaml"))),paste0("phase: validation\ncondition: ",condition,"\nspec_paths:\n  - ",item[[2L]],"\nrequirement: ",requirement,"\ncontext:\n",context_yaml)))
    checked <- yamaa_check(file.path(case,"spec.yaml"))$issues
    rejected <- yamaa_domain(file.path(case,"spec.yaml"))
    stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,condition),
      identical(checked$requirement,requirement),identical(checked$spec_paths,list(item[[2L]])),
      identical(checked$context,context),identical(rejected$issues,checked),is.null(rejected$output),
      is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
      inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"input")))
  }
  cat("public complete date operations match original independent outputs and four complete static failures without Python passed\n")
})

local({
  directory <- tempfile("public-imputation-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("schema-birth-date","sdtm-dm-age")) {
    stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
    stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
    result <- yamaa_domain(entry)
    output <- if(case_name=="schema-birth-date") "adsl.csv" else "dm.csv"
    expected <- read.csv(file.path(case,"expected",output),colClasses="character",na.strings="",check.names=FALSE)
    dates <- if(case_name=="schema-birth-date") c("REFDT","DOB_FIRST","DOB_LAST","DTMDT") else "RFICDTC"
    for(column in dates) expected[[column]] <- as.Date(expected[[column]])
    if(case_name=="schema-birth-date") expected$DTM <- as.POSIXct(expected$DTM,format="%Y-%m-%dT%H:%M:%S",tz="UTC")
    column <- if(case_name=="schema-birth-date") "DOBDY" else "AGE"
    expected[[column]] <- structure(expected[[column]],class="yamaa_int64_vector")
    stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
      !file.exists(file.path(case,output)),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,output)),rawfile(file.path(case,"expected",output))))
  }
  failures <- read.delim(file.path(root,"temporal-imputation-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  stopifnot(nrow(failures)==10L)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    stopifnot(file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    static <- finding$phase=="validation"
    if(static) unlink(file.path(case,"input"),recursive=TRUE)
    checked <- yamaa_check(entry)$issues
    result <- yamaa_domain(entry);actual <- result$issues
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),
      identical(actual$condition,finding$condition),identical(actual$requirement,finding$requirement),
      identical(actual$spec_paths,list(finding$spec_path)),identical(actual$context,finding$context),
      is.null(result$output),identical(is.null(get(".domain_observations",asNamespace("yamaa"))(result)),static),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),length(list.files(case,pattern="[.]csv$"))==0L)
    if(static) stopifnot(identical(checked,actual)) else stopifnot(nrow(checked)==0L)
  }
  cat("public temporal imputation preserves two original outputs and ten complete failures without Python passed\n")
})

local({
  directory <- tempfile("public-string-case-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  source <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
    "output: {path: output.csv, columns: [ID, UP, LOW, SENT, TITLE]}\ncolumns:\n",
    "  - {name: ID, type: str, derivation: SRC.ID}\n",
    "  - {name: UP, type: str, derivation: {str_case: {source: SRC.TEXT, to: upper, missing: MiXeD}}}\n",
    "  - {name: LOW, type: str, derivation: {str_case: {source: SRC.TEXT, to: lower, missing: null}}}\n",
    "  - {name: SENT, type: str, derivation: {str_case: {source: SRC.TEXT, to: sentence, missing: null}}}\n",
    "  - {name: TITLE, type: str, derivation: {str_case: {source: SRC.TEXT, to: title, missing: null}}}\n")
  entry <- file.path(directory,"spec.yaml");writeBin(charToRaw(source),entry)
  stopifnot(nrow(yamaa_check(entry)$issues)==0L)
  writeBin(charToRaw("ID,TEXT\n1,FOLLOW-UP\n2,\u00df\u0130\u00e9\U0001f642AbC\n3,\n"),file.path(directory,"input.csv"))
  result <- yamaa_domain(entry)
  expected_bytes <- charToRaw(paste0("ID,UP,LOW,SENT,TITLE\n1,FOLLOW-UP,follow-up,Follow-up,Follow-Up\n",
    "2,\u00df\u0130\u00e9\U0001f642ABC,\u00df\u0130\u00e9\U0001f642abc,\u00df\u0130\u00e9\U0001f642abc,\u00df\u0130\u00e9\U0001f642Abc\n3,MiXeD,,,\n"))
  expected <- data.frame(ID=c("1","2","3"),UP=c("FOLLOW-UP","\u00df\u0130\u00e9\U0001f642ABC","MiXeD"),
    LOW=c("follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    SENT=c("Follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    TITLE=c("Follow-Up","\u00df\u0130\u00e9\U0001f642Abc",NA_character_),stringsAsFactors=FALSE)
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),!file.exists(file.path(directory,"output.csv")),
    identical(result$save(),TRUE),identical(rawfile(file.path(directory,"output.csv")),expected_bytes))
  source <- sub("to: upper, missing: MiXeD","to: upper",source,fixed=TRUE)
  writeBin(charToRaw(source),entry);writeBin(charToRaw("ID,TEXT\nrow1,\n"),file.path(directory,"input.csv"))
  rejected <- yamaa_domain(entry);actual <- rejected$issues
  # The complete independent reference lives in text-case-reference.json; this
  # literal is its mapping failure with the independently authored UP path.
  stopifnot(nrow(actual)==1L,identical(actual$phase,"mapping"),identical(actual$condition,"missing_input"),
    identical(actual$requirement,"REQ-0334"),identical(actual$spec_paths,list("columns.UP.derivation.str_case")),
    identical(actual$context,'{"keys":[{"ID":"row1"}],"variable":"SRC.TEXT"}'),is.null(rejected$output),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-case-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  stopifnot(nrow(failures)==2L)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    stopifnot(file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    unlink(file.path(case,"input"),recursive=TRUE)
    checked <- yamaa_check(entry)$issues;rejected <- yamaa_domain(entry);actual <- rejected$issues
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),identical(actual$condition,finding$condition),
      identical(actual$requirement,finding$requirement),identical(actual$spec_paths,list(finding$spec_path)),
      identical(actual$context,finding$context),identical(checked,actual),is.null(rejected$output),
      is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
      inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"input")))
  }
  case_name <- "schema-sentence-case"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  checked <- yamaa_check(entry)$issues;result <- yamaa_domain(entry)
  stopifnot(nrow(checked)==0L,nrow(result$issues)==0L,!is.null(result$output),
    !file.exists(file.path(case,"adlb.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adlb.csv")),rawfile(file.path(case,"expected","adlb.csv"))))
  cat("public ASCII string case matches independent exact outputs, original negatives and original ordered sentence artifact without Python passed\n")
})

local({
  directory <- tempfile("public-source-positions-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  entry <- file.path(directory,"spec.yaml");data <- file.path(directory,"input.csv");output <- file.path(directory,"output.csv")
  writeLines(c("schema_version: '1.0'","domain: TEST","keys: [ID]",
    "input: {SRC: {path: input.csv, ordinal: REC}}", "filter: \"SRC.KEEP = 'Y'\"",
    "output: {path: output.csv, columns: [ID, POSITION], order_by: [SORT]}", "columns:",
    "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: POSITION, type: int, derivation: SRC.REC}",
    "  - {name: SORT, type: int, derivation: SRC.SORT}"),entry,useBytes=TRUE)
  inputs <- c('ID,SORT,KEEP,NOTE\na,5,N,first\nb,2,Y,"two\nlines"\nc,1,Y,last\n',"ID,SORT,KEEP,NOTE\n")
  expected <- c("ID,POSITION\nc,3\nb,2\n","ID,POSITION\n")
  for(i in seq_along(inputs)) {
    writeBin(charToRaw(inputs[[i]]),data)
    stopifnot(nrow(yamaa_check(entry)$issues)==0L)
    result <- yamaa_domain(entry)
    stopifnot(nrow(result$issues)==0L,!is.null(result$output),!file.exists(output))
    if(i==1L) stopifnot(identical(result$output$ID,c("c","b")),identical(as.character(result$output$POSITION),c("3","2")))
    else stopifnot(nrow(result$output)==0L)
    unlink(data)
    for(attempt in seq_len(2L)) stopifnot(identical(result$save(),TRUE),identical(rawfile(output),charToRaw(expected[[i]])))
    unlink(output)
  }
  writeBin(charToRaw("ID,REC,SORT,KEEP\n"),data)
  result <- yamaa_domain(entry);actual <- result$issues
  stopifnot(nrow(actual)==1L,identical(actual$phase,"validation"),identical(actual$condition,"ordinal_field_collision"),
    identical(actual$requirement,"REQ-1294"),identical(actual$spec_paths,list("input.SRC.ordinal")),
    identical(actual$context,'{"dataset":"SRC","field":"REC"}'),is.null(result$output),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(output))
  cat("public raw source filter retains multiline record positions and internal presentation order with independent exact cached CSV twice without Python passed\n")
})

local({
  directory <- tempfile("public-string-padding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  cases <- file.path(root,"text-pad-cases")
  findings <- read.delim(file.path(root,"text-pad-findings.tsv"),quote="",colClasses="character",check.names=FALSE)
  for(name in list.dirs(cases,full.names=FALSE,recursive=FALSE)) {
    stopifnot(file.copy(file.path(cases,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    if(name=="width-noninteger") unlink(file.path(case,"input.csv"))
    result <- yamaa_domain(entry)
    expected_csv <- file.path(case,"expected.csv")
    if(file.exists(expected_csv)) {
      stopifnot(nrow(result$issues)==0L,!is.null(result$output),!file.exists(file.path(case,"output.csv")),
        identical(result$save(),TRUE),identical(rawfile(file.path(case,"output.csv")),rawfile(expected_csv)))
    } else {
      expected <- findings[findings$case==name,]
      actual <- result$issues
      stopifnot(nrow(actual)==nrow(expected),is.null(result$output),identical(actual$phase,expected$phase),
        identical(actual$condition,expected$condition),identical(actual$requirement,expected$requirement),
        identical(actual$context,expected$context))
      for(index in seq_len(nrow(expected))) {
        # Each captured padding cause has one independently authored full path.
        stopifnot(identical(paste0('["',actual$spec_paths[[index]],'"]'),expected$spec_paths[[index]]))
      }
      stopifnot(inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
      if(name=="width-noninteger") stopifnot(identical(yamaa_check(entry)$issues,actual),
        is.null(get(".domain_observations",asNamespace("yamaa"))(result)),!file.exists(file.path(case,"input.csv")))
    }
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    if(!is.null(observed)) {
      report <- rawToChar(observed)
      # Literal complete arrays avoid locale-dependent regexp offsets for UTF-8
      # report bytes on Windows and retain the independent empty-ledger truth.
      stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
        sum(gregexpr('"handler_counts":[],',report,fixed=TRUE)[[1L]]>0L)==2L)
    }
  }
  stopifnot(file.copy(file.path(root,"cases","negative-str-pad-width"),directory,recursive=TRUE))
  case <- file.path(directory,"negative-str-pad-width");unlink(file.path(case,"input"),recursive=TRUE)
  entry <- file.path(case,"spec.yaml");checked <- yamaa_check(entry)$issues;rejected <- yamaa_domain(entry)
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"invalid_field_type"),
    identical(checked$requirement,"REQ-0287"),identical(checked$spec_paths,list("columns.IDVARVAL.derivation.str_pad.width")),
    identical(checked$context,'{"actual":"str","expected":"int"}'),identical(rejected$issues,checked),
    is.null(rejected$output),!file.exists(file.path(case,"input")))
  cat("public canonical scalar padding matches complete independent CSV and findings with missing passthrough, no handlers and original width failure before input without Python passed\n")
})

local({
  source(file.path(root,"flag-truth.R"),local=TRUE)
  stopifnot(length(flag_truth)==33L)
  directory <- tempfile("flag-originals-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  observations <- get(".domain_observations",asNamespace("yamaa"))
  for(name in names(flag_truth)) {
    truth <- flag_truth[[name]]
    stopifnot(file.copy(file.path(root,truth$directory,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    stopifnot(identical(rawfile(entry),charToRaw(enc2utf8(truth$specification))))
    files <- list.files(case,recursive=TRUE,full.names=TRUE)
    retained <- lapply(files,rawfile)
    output <- file.path(case,truth$output)
    for(attempt in seq_len(2L)) {
      stopifnot(!file.exists(output))
      result <- yamaa_domain(entry)
      check_issue_frame(result$issues,truth$findings)
      wire <- observations(result)
      if(is.null(wire)) stopifnot(identical(truth$handlers,"[]")) else {
        report <- rawToChar(wire)
        ledger <- paste0('"handler_counts":',truth$handlers,',')
        stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
          sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
      }
      if(nzchar(truth$expected_csv)) {
        expected <- rawfile(file.path(case,truth$expected_csv))
        stopifnot(is.data.frame(result$output),isTRUE(result$save()),identical(rawfile(output),expected))
        unlink(output)
      } else stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
      stopifnot(!file.exists(output),identical(lapply(files,rawfile),retained))
      if(truth$schema_failure) stopifnot(is.null(wire))
    }
  }
  cat("declarative flags: six unchanged positives/six negatives and 21 authored complete captures match findings/CSV/handlers on two attempts without Python; schema provenance and two held report/case gates remain explicit\n")
})

local({
  source(file.path(root,"row-intermediate-truth.R"),local=TRUE)
  directory <- tempfile("public-row-intermediates-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(row_intermediate_truth)) {
    truth <- row_intermediate_truth[[name]]
    if(identical(name,"different-row-drivers")) stopifnot(truth$success,length(truth$issues$phase)==0L)
    stopifnot(file.copy(file.path(root,"row-cases",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    result <- yamaa_domain(entry);check_issue_frame(result$issues,truth$issues)
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    if(!is.null(observed)) {
      report <- rawToChar(observed)
      # Compare complete arrays literally; authored paths can contain brackets.
      ledger <- paste0('"handler_counts":',truth$ledger,',')
      stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
        sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
    }
    output <- file.path(case,truth$output)
    stopifnot(!file.exists(output))
    if(truth$success) {
      stopifnot(!is.null(result$output),identical(result$save(),TRUE),
        identical(rawfile(output),rawfile(file.path(case,"expected.csv"))),
        identical(result$save(),TRUE),identical(rawfile(output),rawfile(file.path(case,"expected.csv"))))
    } else {
      stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(output))
    }
    if(name %in% c("unknown-row-driver","omitted-row-driver","different-row-drivers","adam-adlb-end-of-treatment")) {
      check_issue_frame(yamaa_check(entry)$issues,truth$issues)
    }
  }
  original <- file.path(directory,"adam-adlb-supplb-padded-key")
  unlink(file.path(original,"input"),recursive=TRUE)
  stopifnot(nrow(yamaa_check(file.path(original,"spec.yaml"))$issues)==0L,!file.exists(file.path(original,"input")))
  cat("common input record rows match unchanged padded-key bytes, complete independent findings and reached handler ledgers without Python; EOT intermediate/grouped named-source scopes remain gated passed\n")
})

local({
  directory <- tempfile("public-text-extract-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  handler_truth <- read.delim(file.path(root,"text-extract-handlers.tsv"),quote="",colClasses="character",check.names=FALSE)
  check_handlers <- function(result,case_name) {
    report <- get(".domain_observations",asNamespace("yamaa"))(result)
    expected <- handler_truth$handlers[handler_truth$case==case_name]
    if(!is.null(report) && length(expected)) stopifnot(length(expected)==1L,
      grepl(paste0('"handler_counts":',expected),rawToChar(report),fixed=TRUE))
  }
  case_name <- "adam-adae-text-cleanup"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
  stopifnot(file.rename(input,held),nrow(yamaa_check(entry)$issues)==0L,file.rename(held,input))
  result <- yamaa_domain(entry)
  check_handlers(result,case_name)
  expected <- read.csv(file.path(case,"expected","adae.csv"),colClasses="character",na.strings="",check.names=FALSE)
  for(column in c("AESEQ","AEREFNUM")) expected[[column]] <- structure(expected[[column]],class="yamaa_int64_vector")
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
    !file.exists(file.path(case,"adae.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adae.csv")),rawfile(file.path(case,"expected","adae.csv"))))
  case_name <- "negative-str-uncaptured-group"
  stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues;rejected <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"regex_group_out_of_range"),
    identical(checked$requirement,"REQ-0816"),identical(checked$spec_paths,list("columns.SITEID.derivation.str_extract.group")),
    identical(checked$context,'{"group":2,"group_count":1,"pattern":"^CATH-([^-]+)-[0-9]{4}$"}'),
    identical(rejected$issues,checked),is.null(rejected$output),is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),
    inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-extract-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,];case_name <- finding$case
    stopifnot(file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);rejected <- yamaa_domain(file.path(case,"spec.yaml"));actual <- rejected$issues
    check_handlers(rejected,case_name)
    stopifnot(nrow(actual)==1L,identical(actual$phase,finding$phase),identical(actual$condition,finding$condition),
      identical(actual$requirement,finding$requirement),identical(actual$spec_paths,list(finding$spec_path)),identical(actual$context,finding$context),
      is.null(rejected$output),inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
  }
  for(case_name in c("optional-unentered","entered-empty","whole-empty","leftmost-alternative","last-repeated-capture","literal-null","nested-case","row-extract")) {
    stopifnot(file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);result <- yamaa_domain(file.path(case,"spec.yaml"))
    check_handlers(result,case_name)
    stopifnot(nrow(result$issues)==0L,!file.exists(file.path(case,"output.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv"))))
    if(case_name=="optional-unentered") stopifnot(identical(result$output$RESULT,NA_character_))
    if(case_name %in% c("entered-empty","whole-empty")) stopifnot(identical(result$output$RESULT,""))
  }
  cat("public str_extract matches unchanged ADAE bytes and complete independent failures/captures without Python passed\n")
})

local({
  directory <- tempfile("yamaa-template-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
  handlers <- read.delim(file.path(root,"text-template-handlers.tsv"),quote="",colClasses="character",check.names=FALSE)
  paths <- read.delim(file.path(root,"text-template-directories.tsv"),quote="",colClasses="character",check.names=FALSE)
  directories <- setNames(paths$directory,paths$case)
  check_handlers <- function(result,name) {
    observed <- get(".domain_observations",asNamespace("yamaa"))(result)
    expected <- handlers$handlers[handlers$case==name]
    if(!is.null(observed) && length(expected)) stopifnot(length(expected)==1L,
      grepl(paste0('"handler_counts":',expected),rawToChar(observed),fixed=TRUE))
  }
  name <- "adam-adsl-site-parse"
  stopifnot(file.copy(file.path(root,"cases",name),directory,recursive=TRUE))
  case <- file.path(directory,name);entry <- file.path(case,"spec.yaml");written <- rawfile(entry)
  stopifnot(file.rename(file.path(case,"input"),file.path(case,"held")),nrow(yamaa_check(entry)$issues)==0L,
    file.rename(file.path(case,"held"),file.path(case,"input")))
  result <- yamaa_domain(entry);check_handlers(result,name)
  expected <- read.csv(file.path(case,"expected","adsl.csv"),colClasses="character",na.strings="",check.names=FALSE)
  stopifnot(nrow(result$issues)==0L,identical(result$output,expected),identical(rawfile(entry),written),
    !file.exists(file.path(case,"adsl.csv")),identical(result$save(),TRUE),
    identical(rawfile(file.path(case,"adsl.csv")),rawfile(file.path(case,"expected","adsl.csv"))))
  name <- "negative-subject-reference"
  stopifnot(file.copy(file.path(root,"cases",name),directory,recursive=TRUE))
  case <- file.path(directory,name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- yamaa_check(file.path(case,"spec.yaml"))$issues;result <- yamaa_domain(file.path(case,"spec.yaml"))
  stopifnot(nrow(checked)==1L,identical(checked$phase,"validation"),identical(checked$condition,"invalid_string_template"),
    identical(checked$requirement,"REQ-0461"),identical(checked$spec_paths,list("columns.SUBJREF.derivation.str_template")),
    identical(checked$context,'{"placeholder":"SITEID + \':\' + SUBJID","reason":"invalid_placeholder"}'),
    identical(result$issues,checked),is.null(result$output),is.null(get(".domain_observations",asNamespace("yamaa"))(result)),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
  failures <- read.delim(file.path(root,"text-template-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  failures$requirement[!nzchar(failures$requirement)] <- NA_character_
  for(name in unique(failures$case)) {
    stopifnot(file.copy(file.path(root,"text-template-cases",directories[[name]]),directory,recursive=TRUE))
    case <- file.path(directory,name)
    if(directories[[name]]!=name) stopifnot(file.rename(file.path(directory,directories[[name]]),case))
    result <- yamaa_domain(file.path(case,"spec.yaml"));actual <- result$issues
    expected <- failures[failures$case==name,];check_handlers(result,name)
    stopifnot(nrow(actual)==nrow(expected),identical(actual$phase,expected$phase),identical(actual$condition,expected$condition),
      identical(actual$requirement,expected$requirement),identical(actual$spec_paths,lapply(expected$spec_path,identity)),
      identical(actual$context,expected$context),is.null(result$output),
      inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(case,"output.csv")))
  }
  cases <- sort(paths$case)
  for(name in cases) {
    if(name %in% failures$case || name=="nul-value") next
    # NUL scalar materialization is qualified at pure/native-codec and Python
    # boundaries; the existing R character facade does not admit embedded NUL.
    stopifnot(file.copy(file.path(root,"text-template-cases",directories[[name]]),directory,recursive=TRUE))
    case <- file.path(directory,name)
    if(directories[[name]]!=name) stopifnot(file.rename(file.path(directory,directories[[name]]),case))
    result <- yamaa_domain(file.path(case,"spec.yaml"));check_handlers(result,name)
    stopifnot(nrow(result$issues)==0L,!file.exists(file.path(case,"output.csv")),identical(result$save(),TRUE),
      identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv"))))
    if(name=="empty-template") stopifnot(identical(result$output$RESULT,""))
    if(name=="empty-string-value") stopifnot(identical(result$output$RESULT,"<>"))
    if(name=="missing-null") stopifnot(identical(result$output$RESULT,NA_character_))
  }
  cat("public str_template matches unchanged site bytes and complete ordered independent findings/handlers without Python passed\n")
})

local({
  source(file.path(root,"key-truth.R"),local=TRUE)
  stopifnot(length(key_truth)==19L)
  directory <- tempfile("public-intermediate-keys-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(key_truth)) {
    truth <- key_truth[[name]]
    stopifnot(file.copy(file.path(root,"keys",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);output <- file.path(case,"out.csv")
    for(attempt in seq_len(2L)) {
      stopifnot(!file.exists(output))
      result <- yamaa_domain(entry);check_issue_frame(result$issues,truth$issues)
      observed <- get(".domain_observations",asNamespace("yamaa"))(result)
      if(is.null(observed)) {
        stopifnot(identical(truth$handlers,"[]"))
      } else {
        report <- rawToChar(observed)
        ledger <- paste0('"handler_counts":',truth$handlers,',')
        stopifnot(sum(gregexpr('"handler_counts":',report,fixed=TRUE)[[1L]]>0L)==2L,
          sum(gregexpr(ledger,report,fixed=TRUE)[[1L]]>0L)==2L)
      }
      if(truth$success) {
        stopifnot(!is.null(result$output),identical(result$save(),TRUE),
          identical(rawfile(output),rawfile(file.path(case,"expected.csv"))))
        unlink(output)
      } else {
        stopifnot(is.null(result$output),inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
      }
      stopifnot(!file.exists(output),identical(rawfile(entry),written))
    }
  }
  cat("all 19 unchanged scalar-key captures match complete ordered findings, CSV and handler ledgers twice without Python passed\n")
})

# Compare complete independently authored ledgers while ignoring only JSON object
# key order. Array order, every field, duplicate fields and integer text remain
# part of the comparison; no semantic observation is selected or discarded.
check_handler_ledger <- function(report, expected) {
  string <- '"(?:[^"\\\\]|\\\\.)*"'
  field <- paste0(string,':(?:',string,'|0|[1-9][0-9]*)')
  object <- paste0('\\{',field,'(?:,',field,')*\\}')
  array <- paste0('\\[(?:',object,'(?:,',object,')*)?\\]')
  canonical <- function(value) {
    stopifnot(grepl(paste0('^',array,'$'),value,perl=TRUE))
    objects <- regmatches(value,gregexpr(object,value,perl=TRUE))[[1L]]
    ordered <- vapply(objects,function(value) {
      fields <- regmatches(value,gregexpr(field,value,perl=TRUE))[[1L]]
      paste0('{',paste(sort(fields,method="radix"),collapse=','),'}')
    },character(1L),USE.NAMES=FALSE)
    paste0('[',paste(ordered,collapse=','),']')
  }
  prefix <- '"handler_counts":'
  ledgers <- regmatches(report,gregexpr(paste0(prefix,array),report,perl=TRUE))[[1L]]
  stopifnot(sum(gregexpr(prefix,report,fixed=TRUE)[[1L]]>0L)==2L,
    length(ledgers)==2L,
    identical(vapply(substring(ledgers,nchar(prefix)+1L),canonical,character(1L),USE.NAMES=FALSE),
      rep(canonical(expected),2L)))
}
local({
  expected <- '[{"spec_path":"rows[1].V","handler":"multiple_matches","count":2}]'
  observed <- '[{"count":2,"handler":"multiple_matches","spec_path":"rows[1].V"}]'
  report <- function(value) paste0('{"handler_counts":',value,',"node":{"handler_counts":',value,'}}')
  check_handler_ledger(report(observed),expected)
  for(value in c(sub('"count":2','"count":1',observed,fixed=TRUE),
    sub(',"handler":"multiple_matches"','',observed,fixed=TRUE),
    sub('"count":2','"count":2,"extra":0',observed,fixed=TRUE),
    sub('"count":2','"count":2,"count":2',observed,fixed=TRUE),
    paste0('[',substring(observed,2L,nchar(observed)-1L),',',substring(observed,2L,nchar(observed)-1L),']'))) {
    stopifnot(inherits(tryCatch(check_handler_ledger(report(value),expected),error=identity),"error"))
  }
  ordered <- '[{"count":1,"handler":"no_match","spec_path":"A"},{"count":2,"handler":"no_match","spec_path":"B"}]'
  reversed <- '[{"count":2,"handler":"no_match","spec_path":"B"},{"count":1,"handler":"no_match","spec_path":"A"}]'
  stopifnot(inherits(tryCatch(check_handler_ledger(report(reversed),ordered),error=identity),"error"))
})

local({
  body <- "schema_version: \"1.0\"\ndomain: TEST\ninput:\n  SRC: {path: src.csv}\n  OTHER: {path: other.csv}\nkeys: [ID, SECTION]\noutput: {path: result.csv, columns: [ID, SECTION, K, SEQ, V, FINAL, FIRST]}\nintermediates:\n  - id: P\n    dataset: SELF\n    key: {K: K}\n    order_by: [SELF.SEQ]\n    keep: last\n    no_match: 0\n  - id: F\n    dataset: SELF\n    key: [K]\n    filter: \"SECTION = 'first'\"\n    order_by: [SEQ]\n    keep: last\n    no_match: null\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: SECTION, type: str}\n  - {name: K, type: str, derivation: SRC.K}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: V, type: int, derivation: SRC.V}\n  - {name: FINAL, type: int, derivation: P.V}\n  - {name: FIRST, type: int, derivation: F.V}\nrows:\n  - id: collected\n    dataset: SRC\n    derivations:\n      SECTION: {literal: first}\n  - id: grouped\n    dataset: SRC\n    group_by: [SRC.K]\n    derivations:\n      ID: {literal: g}\n      SECTION: {literal: second}\n      SEQ: {literal: 4}\n      V: {compute: {expr: \"P.V + 10\"}}\n  - id: added\n    dataset: OTHER\n    derivations:\n      ID: OTHER.ID\n      SECTION: {literal: third}\n      K: OTHER.K\n      SEQ: {literal: 9}\n      V: {compute: {expr: \"P.V + 100\"}}\n"
  directory <- tempfile("yamaa-self-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n"),file.path(directory,"src.csv"))
  writeBin(charToRaw("ID,K\nc,x\nd,x\n"),file.path(directory,"other.csv"))
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues)==0L,identical(result$save(),TRUE),identical(rawfile(file.path(directory,"result.csv")),charToRaw("ID,SECTION,K,SEQ,V,FINAL,FIRST\na,first,x,2,5,115,5\nb,first,x,1,9,115,5\ng,second,x,4,15,115,5\nc,third,x,9,115,115,5\nd,third,x,9,115,115,5\n")))
  observed <- rawToChar(get(".domain_observations",asNamespace("yamaa"))(result))
  check_handler_ledger(observed,"[{\"spec_path\":\"rows[1].derivations.V.compute.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows[2].derivations.V.compute.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":2},{\"spec_path\":\"columns.FINAL.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5},{\"spec_path\":\"columns.FIRST.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5}]")

  unlink(file.path(directory,c("src.csv","other.csv","result.csv")))
  writeBin(charToRaw(sub("SECTION: {literal: first}","SECTION: {source: {variable: P.K}}",body,fixed=TRUE)),entry)
  expected <- list(phase="validation",condition="phase_boundary",requirement="REQ-0120",spec_paths=list("rows[0].derivations.SECTION.source"),context="{\"available_phase\":\"completed_prior_row_template\",\"identifier\":\"P.K\",\"required_phase\":\"row_construction\"}")
  check_issue_frame(yamaa_check(entry)$issues,expected)
  rejected <- yamaa_domain(entry);check_issue_frame(rejected$issues,expected)
  stopifnot(is.null(rejected$output),is.null(get(".domain_observations",asNamespace("yamaa"))(rejected)),inherits(tryCatch(rejected$save(),error=identity),"yamaa_domain_error"),!file.exists(file.path(directory,"src.csv")),!file.exists(file.path(directory,"result.csv")))
  cat("completed-template SELF borrowing, complete handlers and pre-study phase refusal passed without Python\n")
})

stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
