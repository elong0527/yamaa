library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"producer-graph")
namespace <- asNamespace("yamaa")
private <- function(name) get(name, namespace)
root <- system.file("project-original", package="yamaa", mustWork=TRUE)
.producer_witness <- new.env(parent=emptyenv())
.producer_witness$index <- 0L
# Private installed-run evidence: the run's complete observations as JSON
# text, or NULL for a specification refused before its run began.
observations <- function(result) private(".run_observations")(result)
observe <- function(result) {
  text <- observations(result)
  output <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  if(nzchar(output)) {
    # Reports remain complete actual native JSON. Disk targets are separate
    # witnesses, preserving both earlier writes and failed-target prior bytes.
    .producer_witness$index <- .producer_witness$index+1L
    directory <- file.path(output,"producer-witnesses/r",sprintf("%03d",.producer_witness$index))
    dir.create(directory,recursive=TRUE,showWarnings=FALSE)
    writeBin(charToRaw(text),file.path(directory,"report.json"))
    saved <- character()
    if(exists("work",envir=parent.frame(),inherits=TRUE)) {
      current <- get("work",envir=parent.frame(),inherits=TRUE)
      for(name in c("generated","published")) {
        paths <- list.files(file.path(current,name),pattern="[.]csv$",full.names=TRUE)
        for(path in paths[!dir.exists(paths)]) {
          relative <- paste0(name,"/",basename(path))
          target <- file.path(directory,"targets",relative)
          dir.create(dirname(target),recursive=TRUE,showWarnings=FALSE)
          stopifnot(file.copy(path,target,overwrite=TRUE))
          saved <- c(saved,relative)
        }
      }
    }
    # This fixture authors fixed filenames without JSON quoting characters.
    targets <- paste0('"',saved,'"',collapse=",")
    if(!length(saved)) targets <- ""
    writeBin(charToRaw(paste0('{"scope":"supplemental_producer_publication",',
      '"truth":"R/yamaa/tests/public_producers.R","report":"report.json",',
      '"saved_targets":[',targets,']}\n')),file.path(directory,"witness.json"))
    provenance <- Sys.getenv("YAMAA_SUPPLEMENTAL_EVIDENCE")
    if(nzchar(provenance)) writeBin(charToRaw(provenance),file.path(directory,"provenance.json"))
  }
  text
}
# The graph a run entered, in entry order: each node's specification, relative
# to the study, and its outcome. This replaces the retired producer evidence.
entered <- function(result,work) {
  nodes <- jsonlite::fromJSON(observations(result),simplifyVector=FALSE)$nodes
  vapply(nodes,function(node) paste0(sub(paste0(work,"/"),"",node$specification,fixed=TRUE),
    ":",node$outcome),character(1))
}
accepted <- function(result) grepl('"outcome":"success"',observations(result),fixed=TRUE) &&
  identical(jsonlite::fromJSON(observations(result),simplifyVector=FALSE)$outcome,"success")
# The execution evidence of a run without its latest publication attempt.
execution <- function(result) sub('"publication":.+?,"report_version":','"report_version":',observe(result),perl=TRUE)
# save() returns invisible NULL; a refused save signals yamaa_error carrying
# the run's own issues.
saved <- function(result) is.null(result$save())
save_refused <- function(result,message) {
  failure <- tryCatch({result$save();NULL},error=identity)
  inherits(failure,"yamaa_error") && identical(conditionMessage(failure),message) &&
    identical(failure$issues,result$issues)
}
failed_save <- function(result) save_refused(result,"cannot save a failed run; see issues")
refused_save <- function(result) save_refused(result,"nothing was saved; see issues")
raw <- fixture_bytes(host_truth[["raw_csv"]])
# Shared authored study truth; R owns its project, environment and capability assertions.
producer <- host_truth[["producer_yaml"]]
consumer <- host_truth[["consumer_yaml"]]
definition <- sub("HOST_CALLABLE","yamaabenchmarks::project_value",host_truth[["function_yaml"]],fixed=TRUE)
write <- function(work, name, value) {
  path <- file.path(work, name); dir.create(dirname(path), recursive=TRUE, showWarnings=FALSE)
  writeBin(if(is.raw(value)) value else charToRaw(value), path)
}
environment_yaml <- "schema_version: '1.0'\nlock: renv.lock\nfunctions: {id_float: float.yaml}\n"
study <- function() {
  work <- tempfile("public-producer-"); dir.create(work)
  work <- normalizePath(work, winslash="/")
  write(work,"yamaa-project.yaml","version: '1.0'\n")
  # The project lists the consumer and links the R environment of functions.
  write(work,"project.yaml","schema_version: '1.0'\nenvironments: {r: env/environment.yaml}\nadam: {specs: [consumer/root.yaml]}\n")
  write(work,"producer/layers/base.yaml",producer)
  write(work,"producer/p.yaml",host_truth[["producer_parent_yaml"]])
  write(work,"consumer/root.yaml",consumer)
  write(work,"raw/raw.csv",raw)
  write(work,"env/environment.yaml",environment_yaml)
  write(work,"env/float.yaml",definition)
  stopifnot(file.copy(file.path(root,"schema-functions/r/renv.lock"),file.path(work,"env/renv.lock")))
  work
}
build <- function(work, name="TEST") yamaa_project(file.path(work,"project.yaml"))$spec(name)$run()
capabilities <- private(".yamaa_locked_host_capabilities")()
with_caps <- function(verify, resolve, code) {
  name <- ".yamaa_locked_host_capabilities"
  original <- get(name, namespace)
  unlockBinding(name, namespace)
  assign(name, function() list(verify=verify,resolve=resolve), namespace)
  lockBinding(name, namespace)
  on.exit({ unlockBinding(name, namespace); assign(name, original, namespace); lockBinding(name, namespace) })
  force(code)
}
work <- study()
write(work,"raw/raw.csv","ID,VALUE\n1,99.99\n")
events <- character()
verify <- function(versions, calls) {
  events <<- c(events,"lock")
  capabilities$verify(versions,calls)
}
resolve <- function(call, parameters) {
  events <<- c(events,"bind")
  resolved <- capabilities$resolve(call,parameters)
  if(resolved[[1L]] != 0L) return(resolved)
  invoke <- resolved[[2L]]
  list(0L,function(arguments) {
    x <- arguments[[1L]]
    events <<- c(events,paste0("invoke:",x))
    if(x==0) write(work,"raw/raw.csv",raw)
    invoke(arguments)
  })
}
results <- with_caps(verify,resolve,lapply(1:2,function(index) build(work)))
stopifnot(identical(events,rep(c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345"),2L)),
  !dir.exists(file.path(work,"generated")))
graph <- c("producer/p.yaml:success","consumer/root.yaml:success")
graph_entered <- lapply(results,entered,work=work)
unlink(work,recursive=TRUE); invisible(gc())
# The entry's run report names each linked input with the byte length of the
# held producer artifact it read (REQ-1176), here the produced CSV.
produced_bytes <- length(fixture_bytes(host_truth[["produced_csv"]]))
linked_details <- c(
  sprintf('{"bytes":%d,"path":"../generated/producer.csv","profile":"csv","schema":"../producer/p.yaml"}',produced_bytes),
  sprintf('{"bytes":%d,"path":"../generated/producer.csv","profile":"csv","schema":"../producer/./p.yaml"}',produced_bytes))
for(index in seq_along(results)) {
  result <- results[[index]]
  stopifnot(identical(result$output$ID,c(1,2)),
    identical(result$output$VALUE,c(2.46,4.70)),nrow(result$issues)==0L,
    identical(result$report$KIND,c("run","input","input","rows","column","column")),
    identical(result$report$OUTCOME[[1L]],"succeeded"),
    identical(result$report$DETAILS[result$report$KIND=="input"],linked_details),
    identical(graph_entered[[index]],graph),accepted(result))
  before <- observe(result)
  observed <- jsonlite::fromJSON(before,simplifyVector=FALSE)
  reads <- vapply(observed$source_reads,function(read) paste0(read$path,":",read$outcome,":",read$snapshots_created),character(1))
  stopifnot(identical(reads,"../raw/raw.csv:captured:1"),length(observed$diagnostics)==0L,
    grepl('"outcome":"success"',before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["raw_first_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["raw_second_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["produced_first_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["produced_second_f64"]],'"'),before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,1.23\\n2,2.35\\n"',before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,2.46\\n2,4.70\\n"',before,fixed=TRUE),
    grepl('"invoked":false',before,fixed=TRUE))
  stopifnot(refused_save(result),
    grepl('"entered":[]',observe(result),fixed=TRUE),
    identical(result$output$VALUE,c(2.46,4.70)),accepted(result))
}
cat("R public aliases, fresh activation, held rounded ingestion and deleted target refusal passed\n")

work <- study()
text <- sub("output: {path:",host_truth[["root_unrounded_column"]],consumer,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",host_truth[["root_unrounded_projection"]],text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
result <- build(work)
stopifnot(nrow(result$issues)==0L,identical(names(result$output),fixture_strings(host_truth[["root_unrounded_columns"]])),
  identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)),
  grepl('"content":"ROOT_UNROUNDED,ID,VALUE\\n2.46,1,2.46\\n4.70,2,4.70\\n"',observe(result),fixed=TRUE))
copy <- result$output; copy$VALUE <- copy$VALUE*10
stopifnot(identical(result$output$VALUE,c(2.46,4.70)))
dir.create(file.path(work,"generated"))
before <- observe(result)
unlink(file.path(work,fixture_strings(host_truth[["owned_directories"]])),recursive=TRUE)
invisible(gc())
for(index in 1:2) {
  stopifnot(saved(result),nrow(result$issues)==0L,
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["root_unrounded_csv"]])),
    grepl('"entered":[{"node":1,"published":true},{"node":0,"published":true}]',observe(result),fixed=TRUE),
    identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)),
    identical(execution(result),before))
}
unlink(work,recursive=TRUE)
stopifnot(identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)))
cat("R public unrounded root precision, projection order and copied views passed\n")

work <- study()
bridge <- host_truth[["retained_bridge_yaml"]]
write(work,"bridge/left.yaml",bridge)
write(work,"bridge/right.yaml",sub("generated/left.csv","generated/right.csv",sub("schema: ../producer/p.yaml","schema: ../producer/./p.yaml",bridge,fixed=TRUE),fixed=TRUE))
text <- sub("../generated/producer.csv, schema: ../producer/p.yaml",host_truth[["nested_left_reference"]],sub(", no_match: null","",consumer,fixed=TRUE),fixed=TRUE)
text <- sub("../generated/producer.csv, schema: ../producer/./p.yaml",host_truth[["nested_right_reference"]],text,fixed=TRUE)
text <- sub("path: ../generated/root.csv","path: ../../generated/root.csv",text,fixed=TRUE)
write(work,"consumer/layers/base.yaml",text)
write(work,"consumer/root.yaml",host_truth[["root_parent_yaml"]])
events <- character()
result <- with_caps(verify,resolve,build(work,"ROOT"))
stopifnot(nrow(result$issues)==0L,identical(result$output$VALUE,c(2.46,4.70)),
  identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
  identical(entered(result,work),c("producer/p.yaml:success","bridge/left.yaml:success",
    "bridge/right.yaml:success","consumer/root.yaml:success")),!dir.exists(file.path(work,"generated")))
dir.create(file.path(work,"generated"))
before <- observe(result)
unlink(file.path(work,fixture_strings(host_truth[["owned_bridge_directories"]])),recursive=TRUE)
invisible(gc())
for(index in 1:2) {
  stopifnot(saved(result),nrow(result$issues)==0L,
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/left.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/right.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])),
    grepl('"entered":[{"node":3,"published":true},{"node":1,"published":true},{"node":2,"published":true},{"node":0,"published":true}]',observe(result),fixed=TRUE),
    identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
    identical(execution(result),before))
}
unlink(work,recursive=TRUE)
stopifnot(identical(result$output$VALUE,c(2.46,4.70)))
cat("R public inherited diamond saves the shared leaf once in producer order without replay\n")

work <- study()
bad <- sub("params: [{name: x","params: [{name: z",definition,fixed=TRUE)
bad <- gsub("args: {x:","args: {z:",bad,fixed=TRUE)
bad <- sub("short-circuit-missing:x","short-circuit-missing:z",bad,fixed=TRUE)
write(work,"env/bad.yaml",bad)
write(work,"env/environment.yaml","schema_version: '1.0'\nlock: renv.lock\nfunctions: {id_float: float.yaml, bad: bad.yaml}\n")
write(work,"consumer/root.yaml",sub(host_truth[["sum_derivation"]],host_truth[["bad_derivation"]],consumer,fixed=TRUE))
unlink(file.path(work,"raw"),recursive=TRUE)
events <- character()
failed <- with_caps(verify,resolve,build(work))
stopifnot(is.null(failed$output),identical(failed$issues$condition,"project_environment_invalid"),
  identical(failed$issues$requirement,"REQ-0695"),
  identical(events,c("lock","bind","bind")),length(entered(failed,work))==0L,
  grepl('"source_reads":[]',observe(failed),fixed=TRUE),grepl('"tests":[]',observe(failed),fixed=TRUE))
unlink(work,recursive=TRUE)
cat("R public consumer-only bad binding refuses before all study and runtime effects\n")

for(interrupted in c(FALSE,TRUE)) {
  work <- study()
  failure <- structure(list(message="original public graph host cause",call=NULL),
    class=if(interrupted) c("public_producer_interrupt","interrupt","condition") else c("public_producer_failure","error","condition"))
  resolve_failure <- function(call,parameters) {
    resolved <- capabilities$resolve(call,parameters)
    if(resolved[[1L]] != 0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L,function(arguments) {
      x <- arguments[[1L]]
      if(x==1.234) return(list(if(interrupted) 3L else 1L,failure))
      invoke(arguments)
    })
  }
  if(interrupted) {
    # An interrupted run signals the original interrupt condition itself.
    seen <- with_caps(capabilities$verify,resolve_failure,tryCatch(build(work),interrupt=identity))
    stopifnot(identical(seen,failure))
    unlink(work,recursive=TRUE); invisible(gc())
  } else {
    owned <- with_caps(capabilities$verify,resolve_failure,build(work))
    graph <- entered(owned,work)
    unlink(work,recursive=TRUE); invisible(gc())
    context <- jsonlite::fromJSON(owned$issues$context[[1L]],simplifyVector=FALSE)
    stopifnot(!accepted(owned),identical(graph,"producer/p.yaml:failure"),
      identical(entered(owned,work),graph),is.null(owned$output),
      identical(owned$issues$condition,"function_call_failed"),
      identical(context$host_error,"public_producer_failure"),identical(context$host_message,failure$message))
  }
}
cat("R public original condition/interrupt identity and entered prefixes survive other owners and files\n")

work <- study()
oversized <- structure(list(message=paste0(strrep("\u00e9\n",30000L),"complete producer failure"),call=NULL),class=c("public_producer_error","error","condition"))
resolve_oversized <- function(call,parameters) {
  resolved <- capabilities$resolve(call,parameters)
  if(resolved[[1L]] != 0L) return(resolved)
  invoke <- resolved[[2L]]
  list(0L,function(arguments) {
    if(identical(arguments[[1L]],1.234)) return(list(1L,oversized))
    invoke(arguments)
  })
}
failed <- with_caps(capabilities$verify,resolve_oversized,build(work))
graph <- entered(failed,work)
unlink(work,recursive=TRUE); invisible(gc())
stopifnot(is.null(failed$output),identical(graph,"producer/p.yaml:failure"),
  identical(failed$issues$condition,"function_call_failed"))
context <- jsonlite::fromJSON(failed$issues$context[[1L]],simplifyVector=FALSE)
stopifnot(identical(context$host_error,"public_producer_error"),
  identical(context$host_message,oversized$message),is.null(context$host_details_truncated))
cat("R public long host messages preserve exact Unicode beside the original rooted condition\n")

work <- study()
write(work,"consumer/root.yaml",paste0(consumer,host_truth[["fatal_row_count_yaml"]]))
failed <- build(work)
stopifnot(is.null(failed$output),identical(failed$issues$condition,"row_count_failed"),
  identical(entered(failed,work),c("producer/p.yaml:success","consumer/root.yaml:failure")),
  grepl('"content":"ID,VALUE\\n1,1.23\\n2,2.35\\n"',observe(failed),fixed=TRUE),
  failed_save(failed),!dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE)
stopifnot(is.null(failed$output))
work <- study(); write(work,"raw/raw.csv","ID,VALUE\n1,bad\n")
failed <- build(work)
stopifnot(is.null(failed$output),nrow(failed$issues)>0L,identical(entered(failed,work),"producer/p.yaml:failure"),
  grepl('"artifacts":[]',observe(failed),fixed=TRUE))
unlink(work,recursive=TRUE)
cat("R public ingestion and late verification failures retain evidence without accepted root output\n")

work <- study()
write(work,"consumer/root.yaml",sub("schema: ../producer/p.yaml",host_truth[["incompatible_reference"]],consumer,fixed=TRUE))
failed <- build(work)
stopifnot(is.null(observations(failed)),is.null(failed$report),is.null(failed$output),"REQ-0523" %in% failed$issues$requirement)
before <- failed$issues
unlink(work,recursive=TRUE)
# A refusal before the run began is a fixed snapshot of its issues.
stopifnot(identical(failed$issues,before),failed_save(failed))
# A malformed environment file leaves the project unreadable: every project
# operation refuses with the same finding at the environments.r link.
work <- study(); write(work,"env/environment.yaml","schema_version: [\n")
project <- yamaa_project(file.path(work,"project.yaml"))
unread <- tryCatch(project$spec("TEST"),error=identity)
before <- unread$issues
context <- jsonlite::fromJSON(before$context[[1L]],simplifyVector=FALSE)
refused <- project$run()
stopifnot(inherits(unread,"yamaa_error"),identical(conditionMessage(unread),"cannot read the project; see issues"),
  identical(before$condition,"invalid_yaml"),identical(before$requirement,"REQ-0280"),
  identical(context$environment_path,"environments.r"),identical(context$path,"env/environment.yaml"),
  identical(refused$names,character()),identical(refused$issues,before),identical(project$check()$issues,before))
write(work,"env/environment.yaml",environment_yaml)
write(work,"consumer/root.yaml",sub("schema: ../producer/p.yaml","schema: null",consumer,fixed=TRUE))
# A consumer refused while it is prepared has no domain name, so the project
# run reports its refusal and builds nothing; its save signals the issues.
project <- yamaa_project(file.path(work,"project.yaml"))
rejected <- project$run()
unnamed <- tryCatch(project$spec("TEST"),error=identity)
stopifnot(identical(rejected$names,character()),nrow(rejected$report)==0L,"REQ-0287" %in% rejected$issues$requirement,
  inherits(unnamed,"yamaa_error"),identical(conditionMessage(unnamed),"no specification named TEST; available: "),
  save_refused(rejected,"a specification failed, so no primary artifact was saved; see issues"))
unlink(work,recursive=TRUE)
stopifnot(identical(unread$issues,before),identical(refused$issues,before))
cat("R public metadata and early raw rejections retain original evidence without fallback\n")

# Explicit small copy-policy refusals remain in native producer_result controls.
# The approved longitudinal public defaults admit both original geometries.
# `values` is the ID column as R holds it: str text, or int as double.
assert_large_projection_admitted <- function(result, work, ids, columns, expected, values=ids) {
  stopifnot(accepted(result), !is.null(result$output),
    identical(entered(result,work),c("producer/p.yaml:success","consumer/root.yaml:success")), nrow(result$issues)==0L,
    identical(names(result$output),columns), identical(result$output$ID,values),
    identical(result$output$VALUE,rep(2.46,length(ids))),
    grepl('"outcome":"success"',observe(result),fixed=TRUE),
    !dir.exists(file.path(work,"generated")))
  held <- result$output
  produced <- paste0("ID,VALUE\n",paste0(ids,",1.23\n",collapse=""))
  dir.create(file.path(work,"generated"))
  unlink(file.path(work,fixture_strings(host_truth[["owned_directories"]])),recursive=TRUE)
  invisible(gc())
  for(index in 1:2) {
    stopifnot(saved(result), identical(result$output,held), accepted(result))
    root_path <- file.path(work,"generated/root.csv")
    producer_path <- file.path(work,"generated/producer.csv")
    stopifnot(
      identical(readBin(root_path,"raw",n=file.info(root_path)$size),charToRaw(expected)),
      identical(readBin(producer_path,"raw",n=file.info(producer_path)$size),charToRaw(produced)))
  }
}
work <- study()
write(work,"producer/layers/base.yaml",sub("name: ID, type: int","name: ID, type: str",
  sub("ID: int","ID: str",producer,fixed=TRUE),fixed=TRUE))
extras <- paste0("EXTRA",0:58)
extra <- paste0(vapply(extras,function(name) paste0("  - {name: ",name,
  host_truth[["extra_null_column_suffix"]]),character(1)),collapse="")
columns <- c("ID","VALUE",extras)
text <- sub("output: {path:",paste0(extra,"output: {path:"),consumer,fixed=TRUE)
text <- sub("name: ID, type: int","name: ID, type: str",text,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",paste0("columns: [",paste(columns,collapse=", "),"], decimals: 2"),text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
ids <- as.character(1:23000)
write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0(ids,",1.234\n",collapse="")))
result <- build(work)
stopifnot(!is.null(result$output), all(vapply(extras,function(name)
  identical(result$output[[name]],rep(NA_character_,length(ids))),logical(1))))
expected <- paste0(paste(columns,collapse=","),"\n",paste0(ids,",2.46",strrep(",",length(extras)),"\n",collapse=""))
assert_large_projection_admitted(result,work,ids,columns,expected)
unlink(work,recursive=TRUE); invisible(gc())
cat("R public copies admit original large cell geometry and preserve exact held Saves\n")

work <- study()
payload <- strrep("x",2048)
extra <- paste0(host_truth[["large_payload_column"]],payload,'"}}\n')
text <- sub("output: {path:",paste0(extra,"output: {path:"),consumer,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",host_truth[["payload_projection"]],text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
ids <- as.character(1:4095)
write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0(ids,",1.234\n",collapse="")))
result <- build(work)
stopifnot(!is.null(result$output), identical(result$output$PAYLOAD,rep(payload,length(ids))))
expected <- paste0("ID,VALUE,PAYLOAD\n",paste0(ids,",2.46,",payload,"\n",collapse=""))
assert_large_projection_admitted(result,work,ids,c("ID","VALUE","PAYLOAD"),expected,as.double(ids))
unlink(work,recursive=TRUE); invisible(gc())
cat("R public copies admit original large byte geometry and preserve exact held Saves\n")

work <- study();dir.create(file.path(work,"generated"));dir.create(file.path(work,"generated/root.csv"))
result <- build(work)
stopifnot(refused_save(result),accepted(result),
  identical(result$output$VALUE,c(2.46,4.70)),!file.exists(file.path(work,"generated/producer.csv")),
  grepl('"entered":[]',observe(result),fixed=TRUE))
unlink(file.path(work,"generated/root.csv"),recursive=TRUE)
stopifnot(saved(result),nrow(result$issues)==0L)
unlink(work,recursive=TRUE)
cat("R whole target-set preflight refuses late invalid target before every write\n")

save_failure <- function(failed) {
  work <- study()
  on.exit(unlink(work,recursive=TRUE))
  write(work,"consumer/root.yaml",sub("../generated/root.csv","../published/root.csv",consumer,fixed=TRUE))
  write(work,"generated/producer.csv","previous producer bytes")
  write(work,"published/root.csv","previous consumer bytes")
  events <- character()
  versions <- function(versions,calls) {events <<- c(events,"lock");capabilities$verify(versions,calls)}
  binding <- function(call,parameters) {
    events <<- c(events,"bind");bound <- capabilities$resolve(call,parameters)
    if(bound[[1L]]!=0L) return(bound)
    invoke <- bound[[2L]]
    list(0L,function(arguments) {events <<- c(events,"invoke");invoke(arguments)})
  }
  result <- with_caps(versions,binding,build(work));before <- execution(result);calls <- events
  target <- file.path(work,if(failed=="producer") "generated/producer.csv" else "published/root.csv")
  readonly <- if(.Platform$OS.type=="windows") target else dirname(target)
  Sys.chmod(readonly,if(.Platform$OS.type=="windows") "0444" else "0500")
  on.exit(Sys.chmod(readonly,if(.Platform$OS.type=="windows") "0666" else "0700"),add=TRUE,after=FALSE)
  if(.Platform$OS.type!="windows") {
    probe <- file.path(readonly,".write-permission-probe")
    writable <- suppressWarnings(tryCatch({writeBin(charToRaw("permission probe"),probe);TRUE},error=function(error) FALSE))
    if(writable) {
      unlink(probe)
      cat("R publication permission witness skipped: directory permissions cannot deny writes on this host\n")
      return(FALSE)
    }
  }
  stopifnot(refused_save(result),identical(execution(result),before),identical(events,calls),
    identical(result$output$VALUE,c(2.46,4.70)),"REQ-0764" %in% result$issues$requirement,
    identical(readBin(target,"raw",n=100),charToRaw(if(failed=="producer") "previous producer bytes" else "previous consumer bytes")),
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),charToRaw(if(failed=="producer") "previous producer bytes" else host_truth[["produced_csv"]])),
    grepl(if(failed=="producer") '"entered":[{"node":1,"published":false}]' else '"entered":[{"node":1,"published":true},{"node":0,"published":false}]',observe(result),fixed=TRUE),
    length(list.files(dirname(target),pattern="[.]stage$",all.files=TRUE))==0L)
  Sys.chmod(readonly,if(.Platform$OS.type=="windows") "0666" else "0700")
  stopifnot(saved(result),nrow(result$issues)==0L,identical(events,calls),
    identical(execution(result),before))
  TRUE
}
qualified <- vapply(c("producer","consumer"),save_failure,logical(1))
if(all(qualified)) cat("R actual producer/consumer publication failures preserve original Build, actual prefix and earlier writes\n")

# Two primary files of one graph that resolve to one location collide when
# the graph is validated (REQ-0756): nothing is read, run or written.
refused_at_validation <- function(target) {
  work <- study()
  on.exit(unlink(work,recursive=TRUE))
  dir.create(file.path(work,"generated"))
  write(work,"consumer/root.yaml",sub("../generated/root.csv",target,consumer,fixed=TRUE))
  failed <- build(work)
  stopifnot(is.null(observations(failed)),is.null(failed$report),is.null(failed$output),
    identical(failed$issues$condition,"artifact_path_collision"),
    identical(failed$issues$requirement,"REQ-0756"),
    identical(failed$issues$spec_paths,list(c("output.path","output.path"))),
    failed_save(failed),
    length(list.files(file.path(work,"generated"),all.files=TRUE,no..=TRUE))==0L)
}
refused_at_validation("../generated/dir/../producer.csv")
cat("R graph primary files at one resolved location are refused at validation\n")
# Ordinal case-insensitive names select one Windows target, so the consumer's
# PRODUCER.csv names the producer's producer.csv.
if(.Platform$OS.type=="windows") {
  refused_at_validation("../generated/PRODUCER.csv")
  cat("R absent Windows case-alias targets are refused at validation\n")
}

if(Sys.info()[["sysname"]]=="Darwin") {
  work <- study();dir.create(file.path(work,"generated"))
  write(work,"generated/case-probe","native case witness")
  sensitive <- !file.exists(file.path(work,"generated/CASE-PROBE"))
  unlink(file.path(work,"generated/case-probe"))
  for(name in c("PRODUCER.csv","\\u00e9.csv")) {
    parts <- strsplit(consumer,"../generated/root.csv",fixed=TRUE)[[1L]]
    write(work,"consumer/root.yaml",paste(parts,collapse=paste0('"../generated/',name,'"')))
    result <- build(work)
    stopifnot(!is.null(result$output),accepted(result))
    if(name=="PRODUCER.csv" && sensitive) {
      stopifnot(saved(result),
        identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
        identical(readBin(file.path(work,"generated/PRODUCER.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])))
      unlink(list.files(file.path(work,"generated"),full.names=TRUE))
    } else stopifnot(refused_save(result),
      grepl('"entered":[]',observe(result),fixed=TRUE),
      length(list.files(file.path(work,"generated"),all.files=TRUE,no..=TRUE))==0L)
  }
  unlink(work,recursive=TRUE)
  cat("R macOS parent case policy and unproven Unicode names refuse before publication\n")
}

# Without a project a specification reads no environment, even beside
# malformed environment YAML and an unused project file. Both Check and Run
# use the producer closure without activation authority.
work <- study()
write(work,"producer/layers/base.yaml",sub(host_truth[["identity_derivation"]],"RAW.VALUE",producer,fixed=TRUE))
write(work,"env/environment.yaml","[")
dir.create(file.path(work,"generated"))
poison <- function(...) stop("absent environment entered host activation")
result <- with_caps(poison,poison,{
  built <- yamaa_spec(file.path(work,"consumer/root.yaml"))$run()
  read <- vapply(jsonlite::fromJSON(observations(built),simplifyVector=FALSE)$source_reads,
    function(item) item$path,character(1))
  stopifnot(nrow(built$issues)==0L,identical(built$output$VALUE,c(2.46,4.70)),
    grepl('"environment":{"lock_source":null,"source":null}',observe(built),fixed=TRUE),
    grepl('"lock":"not_requested"',observe(built),fixed=TRUE),
    identical(read,"../raw/raw.csv"))
  unlink(file.path(work,"raw/raw.csv"))
  stopifnot(nrow(yamaa_spec(file.path(work,"consumer/root.yaml"))$check()$issues)==0L)
  unlink(file.path(work,fixture_strings(host_truth[["retained_specification_files"]])))
  for(index in 1:2) stopifnot(saved(built),
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])))
  built
})
unlink(work,recursive=TRUE)
cat("R absent environment graphs capture no environment and repeat Save from held bytes\n")

# Paired public graph witness: seven independently authored records become one
# counted producer row, then one consumer row, through each artifact profile.
# REQ-0047 makes the copied producer count an explicit scalar group key.
for(profile in c("csv","parquet")) {
  work <- study()
  write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0("1,",1:7,"\n",collapse="")))
  write(work,"producer/p.yaml",paste0("schema_version: '1.0'\ndomain: COUNTED\nkeys: [ID]\ninput: {RAW: {path: ../raw/raw.csv, types: {ID: int, VALUE: int}}}\nrows:\n  - id: counted\n    dataset: RAW\n    group_by: [RAW.ID]\n    derivations: {ID: RAW.ID, VALUE: {aggregate: 'COUNT(RAW.*)'}}\ncolumns:\n  - {name: ID, type: int, label: Identifier}\n  - {name: VALUE, type: int, label: Count}\noutput: {path: ../generated/producer.",profile,", columns: [ID, VALUE]}\n"))
  write(work,"consumer/root.yaml",paste0("schema_version: '1.0'\ndomain: ROOT\nkeys: [ID]\ninput: {FIRST: {path: ../generated/producer.",profile,", schema: ../producer/p.yaml}}\nrows:\n  - id: counted\n    dataset: FIRST\n    group_by: [FIRST.ID, FIRST.VALUE]\n    derivations: {ID: FIRST.ID, SOURCE_COUNT: FIRST.VALUE, VALUE: {aggregate: 'COUNT(FIRST.*)'}}\ncolumns:\n  - {name: ID, type: int, label: Identifier}\n  - {name: SOURCE_COUNT, type: int, label: Original source count}\n  - {name: VALUE, type: int, label: Count}\noutput: {path: ../generated/root.csv, columns: [ID, SOURCE_COUNT, VALUE]}\n"))
  result <- yamaa_spec(file.path(work,"consumer/root.yaml"))$run()
  stopifnot(nrow(result$issues)==0L,nrow(result$output)==1L,
    identical(names(result$output),c("ID","SOURCE_COUNT","VALUE")),
    identical(result$output$ID,1),identical(result$output$SOURCE_COUNT,7),
    identical(result$output$VALUE,1))
  dir.create(file.path(work,"generated"),showWarnings=FALSE)
  for(index in 1:2) stopifnot(saved(result),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),charToRaw("ID,SOURCE_COUNT,VALUE\n1,7,1\n")))
  if(profile=="csv") stopifnot(identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),charToRaw("ID,VALUE\n1,7\n")))
  unlink(work,recursive=TRUE)
}
cat("R uneven seven-to-one producer graphs retain independent counts through CSV and Parquet\n")

assert_untyped_date_selector_producer <- function() {
  work <- tempfile("public-producer-dates-")
  dir.create(work)
  work <- normalizePath(work,winslash="/")
  on.exit(unlink(work,recursive=TRUE))
  producer_spec <- "schema_version: '1.0'
domain: ADSL
keys: [USUBJID]
input: {EX: {path: ../raw/ex.csv}}
base: EX
intermediates:
  - id: EXP
    dataset: EX
    derivations:
      EXSTDT: {to_date: {source: EXSTDTC}}
      EXENDT0: {to_date: {source: EXENDTC}}
      EXENDT: {first_available: {sources: [EXENDT0, EXSTDT]}}
    filter: EX.EXENDT >= EX.EXSTDT
    no_match: null
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: EX.USUBJID}
  - {name: TRTSDT, type: date, label: First Dose Date, derivation: EXP.EXSTDT}
  - {name: TRTEDT, type: date, label: Last Dose Date, derivation: EXP.EXENDT}
output: {path: ../generated/producer.csv, columns: [USUBJID, TRTSDT, TRTEDT]}
"
  write(work,"producer/p.yaml",producer_spec)
  write(work,"consumer/root.yaml","schema_version: '1.0'
domain: ADEX
keys: [USUBJID]
input: {ADSL: {path: ../generated/producer.csv, schema: ../producer/p.yaml}}
base: ADSL
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: ADSL.USUBJID}
  - {name: TRTEDT, type: date, label: Last Dose Date, derivation: ADSL.TRTEDT}
output: {path: ../generated/root.csv, columns: [USUBJID, TRTEDT]}
")
  write(work,"raw/ex.csv","USUBJID,EXSTDTC,EXENDTC\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,\n")
  produced <- charToRaw("USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\n")
  consumed <- charToRaw("USUBJID,TRTEDT\nS01,2024-01-10\nS02,2024-01-05\n")
  dir.create(file.path(work,"generated"))
  for(entry in c("producer/p.yaml","consumer/root.yaml")) {
    path <- file.path(work,entry)
    stopifnot(nrow(yamaa_spec(path)$check()$issues)==0L)
    result <- yamaa_spec(path)$run()
    stopifnot(nrow(result$issues)==0L,!is.null(result$output),nrow(result$output)==2L,
      saved(result))
    target <- file.path(work,"generated",if(startsWith(entry,"producer/")) "producer.csv" else "root.csv")
    stopifnot(identical(readBin(target,"raw",n=file.info(target)$size),
      if(startsWith(entry,"producer/")) produced else consumed))
  }
  target <- file.path(work,"generated/producer.csv")
  stopifnot(identical(readBin(target,"raw",n=file.info(target)$size),produced))

  # This filter remains invalid independently of selector result inference.
  write(work,"producer/p.yaml",sub("EX.EXENDT >= EX.EXSTDT","EX.EXSTDT >= 'not-a-date'",producer_spec,fixed=TRUE))
  prior <- list("generated/producer.csv"=charToRaw("previous producer bytes"),
    "generated/root.csv"=charToRaw("previous consumer bytes"))
  for(name in names(prior)) write(work,name,prior[[name]])
  failed <- yamaa_spec(file.path(work,"consumer/root.yaml"))$run()
  issues <- failed$issues
  stopifnot(is.null(failed$output),nrow(issues)==1L,
    identical(issues$phase,"validation"),identical(issues$condition,"incompatible_input_type"),
    identical(issues$requirement,"REQ-0190"),identical(issues$spec_paths,list("input.ADSL.schema.intermediates[0].filter")))
  context <- jsonlite::fromJSON(issues$context[[1L]],simplifyVector=FALSE)
  stopifnot(identical(context[sort(names(context))],list(left_type="date",right_type="str")))
  for(index in 1:2) {
    failure <- tryCatch(failed$save(),error=identity)
    stopifnot(inherits(failure,"yamaa_error"),
      identical(conditionMessage(failure),"cannot save a failed run; see issues"),
      identical(failure$issues,issues),identical(failed$issues,issues))
    for(name in names(prior)) {
      target <- file.path(work,name)
      stopifnot(identical(readBin(target,"raw",n=file.info(target)$size),prior[[name]]))
    }
  }
}
assert_untyped_date_selector_producer()
cat("R public untyped date selectors and linked producer filter causes retain exact saved bytes\n")
