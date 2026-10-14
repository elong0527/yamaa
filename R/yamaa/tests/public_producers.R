library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"producer-graph")
namespace <- asNamespace("yamaa")
private <- function(name) get(name, namespace)
root <- system.file("project-original", package="yamaa", mustWork=TRUE)
reply <- private(".domain_reply")
handle <- function(result) get("h", envir=environment(result$save), inherits=FALSE)
.producer_witness <- new.env(parent=emptyenv())
.producer_witness$index <- 0L
observe <- function(result) {
  bytes <- private(".domain_observations")(result)
  output <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  if(nzchar(output)) {
    # Reports remain complete actual native JSON. Disk targets are separate
    # witnesses, preserving both earlier writes and failed-target prior bytes.
    .producer_witness$index <- .producer_witness$index+1L
    directory <- file.path(output,"producer-witnesses/r",sprintf("%03d",.producer_witness$index))
    dir.create(directory,recursive=TRUE,showWarnings=FALSE)
    writeBin(bytes,file.path(directory,"report.json"))
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
  rawToChar(bytes)
}
evidence <- function(result) reply(.Call(private("wrap__domain_producer_evidence"), handle(result)))
preparation <- function(result) reply(.Call(private("wrap__domain_preparation_sources"), handle(result)))
raw <- fixture_bytes(host_truth[["raw_csv"]])
# Shared authored study truth; R owns its environment and capability assertions.
producer <- host_truth[["producer_yaml"]]
consumer <- host_truth[["consumer_yaml"]]
definition <- sub("HOST_CALLABLE","yamaabenchmarks::project_value",host_truth[["function_yaml"]],fixed=TRUE)
write <- function(work, name, value) {
  path <- file.path(work, name); dir.create(dirname(path), recursive=TRUE, showWarnings=FALSE)
  writeBin(if(is.raw(value)) value else charToRaw(value), path)
}
study <- function() {
  work <- tempfile("public-producer-"); dir.create(work)
  work <- normalizePath(work, winslash="/")
  write(work,"yamaa-project.yaml","version: '1.0'\n")
  write(work,"producer/layers/base.yaml",producer)
  write(work,"producer/p.yaml",host_truth[["producer_parent_yaml"]])
  write(work,"consumer/root.yaml",consumer)
  write(work,"raw/raw.csv",raw)
  write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml}\n")
  write(work,"env/float.yaml",definition)
  stopifnot(file.copy(file.path(root,"schema-functions/r/renv.lock"),file.path(work,"env/renv.lock")))
  work
}
build <- function(work) yamaa_domain(file.path(work,"consumer/root.yaml"), environment=file.path(work,"env/environment.yaml"))
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
  list(0L,function(encoded) {
    x <- private(".scalar_unpack")(encoded[[1L]])
    events <<- c(events,paste0("invoke:",x))
    if(x==0) write(work,"raw/raw.csv",raw)
    invoke(encoded)
  })
}
results <- with_caps(verify,resolve,lapply(1:2,function(index) build(work)))
stopifnot(identical(events,rep(c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345"),2L)),
  !dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE); invisible(gc())
for(result in results) {
  stopifnot(identical(as.character(result$output$ID),c("1","2")),
    identical(result$output$VALUE,c(2.46,4.70)),nrow(result$issues)==0L,
    is.null(result$verification_log),is.null(result$warning_log))
  facts <- evidence(result)
  stopifnot(isTRUE(facts$accepted),identical(unlist(facts$entered),c(1,0)),length(facts$failures)==0L)
  source <- Filter(function(item) endsWith(rawToChar(item$name),"/raw/raw.csv"),facts$sources)
  stopifnot(length(source)==1L,identical(source[[1L]]$bytes,raw))
  before <- observe(result)
  stopifnot(grepl('"outcome":"success"',before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["raw_first_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["raw_second_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["produced_first_f64"]],'"'),before,fixed=TRUE),
    grepl(paste0('"value":"',host_truth[["produced_second_f64"]],'"'),before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,1.23\\n2,2.35\\n"',before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,2.46\\n2,4.70\\n"',before,fixed=TRUE),
    grepl('"invoked":false',before,fixed=TRUE))
  stopifnot(identical(result$save(),FALSE),
    grepl('"entered":[]',observe(result),fixed=TRUE),
    identical(result$output$VALUE,c(2.46,4.70)),isTRUE(evidence(result)$accepted))
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
  stopifnot(isTRUE(result$save()),nrow(result$issues)==0L,
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["root_unrounded_csv"]])),
    grepl('"entered":[{"node":1,"published":true},{"node":0,"published":true}]',observe(result),fixed=TRUE),
    identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)),
    identical(sub('"publication":.+?,"report_version":','"report_version":',observe(result),perl=TRUE),before))
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
result <- with_caps(verify,resolve,build(work))
stopifnot(nrow(result$issues)==0L,identical(result$output$VALUE,c(2.46,4.70)),
  identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
  identical(unlist(evidence(result)$entered),c(3,1,2,0)),!dir.exists(file.path(work,"generated")))
dir.create(file.path(work,"generated"))
before <- observe(result)
unlink(file.path(work,fixture_strings(host_truth[["owned_bridge_directories"]])),recursive=TRUE)
invisible(gc())
for(index in 1:2) {
  stopifnot(isTRUE(result$save()),nrow(result$issues)==0L,
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/left.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/right.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])),
    grepl('"entered":[{"node":3,"published":true},{"node":1,"published":true},{"node":2,"published":true},{"node":0,"published":true}]',observe(result),fixed=TRUE),
    identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
    identical(sub('"publication":.+?,"report_version":','"report_version":',observe(result),perl=TRUE),before))
}
unlink(work,recursive=TRUE)
stopifnot(identical(result$output$VALUE,c(2.46,4.70)))
cat("R public inherited diamond saves the shared leaf once in producer order without replay\n")

work <- study()
bad <- sub("params: [{name: x","params: [{name: z",definition,fixed=TRUE)
bad <- gsub("args: {x:","args: {z:",bad,fixed=TRUE)
bad <- sub("short-circuit-missing:x","short-circuit-missing:z",bad,fixed=TRUE)
write(work,"env/bad.yaml",bad)
write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml, bad: bad.yaml}\n")
write(work,"consumer/root.yaml",sub(host_truth[["sum_derivation"]],host_truth[["bad_derivation"]],consumer,fixed=TRUE))
unlink(file.path(work,"raw"),recursive=TRUE)
events <- character()
failed <- with_caps(verify,resolve,build(work))
stopifnot(is.null(failed$output),identical(failed$issues$condition,"project_environment_invalid"),
  identical(failed$issues$requirement,"REQ-0695"),
  identical(events,c("lock","bind","bind")),length(evidence(failed)$entered)==0L,
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
    list(0L,function(encoded) {
      x <- private(".scalar_unpack")(encoded[[1L]])
      if(x==1.234) return(list(if(interrupted) 3L else 1L,failure))
      invoke(encoded)
    })
  }
  # Native public result ownership is available independently of propagation.
  args <- private(".domain_arguments")(file.path(work,"consumer/root.yaml"),file.path(work,"env/environment.yaml"))
  owned <- reply(.Call(private("wrap__domain_file"),args[[1L]],args[[2L]],args[[3L]],list(capabilities$verify,resolve_failure,private(".project_condition_details"))))
  if(interrupted) {
    seen <- with_caps(capabilities$verify,resolve_failure,tryCatch(build(work),interrupt=identity))
    stopifnot(identical(seen,failure))
  }
  unlink(work,recursive=TRUE); invisible(gc())
  facts <- reply(.Call(private("wrap__domain_producer_evidence"),owned))
  stopifnot(!facts$accepted,identical(unlist(facts$entered),1),length(facts$failures)==1L,
    identical(facts$failures[[1L]]$facts$condition,failure),
    is.null(reply(.Call(private("wrap__domain_output"),owned))))
  if(interrupted) stopifnot(identical(reply(.Call(private("wrap__domain_interrupt"),owned)),failure))
  else stopifnot(identical(reply(.Call(private("wrap__domain_issues"),owned))$condition,"function_call_failed"))
}
cat("R public original condition/interrupt identity and entered prefixes survive other owners and files\n")

work <- study()
oversized <- structure(list(message=strrep("x",65537L),call=NULL),class=c("public_producer_error","error","condition"))
resolve_oversized <- function(call,parameters) {
  resolved <- capabilities$resolve(call,parameters)
  if(resolved[[1L]] != 0L) return(resolved)
  invoke <- resolved[[2L]]
  list(0L,function(encoded) {
    if(private(".scalar_unpack")(encoded[[1L]])==1.234) return(list(1L,oversized))
    invoke(encoded)
  })
}
failed <- with_caps(capabilities$verify,resolve_oversized,build(work))
unlink(work,recursive=TRUE); invisible(gc())
stopifnot(is.null(failed$output),identical(unlist(evidence(failed)$entered),1),
  identical(evidence(failed)$failures[[1L]]$facts$condition,oversized),
  grepl('"code":"report_limit"',failed$issues$context,fixed=TRUE))
cat("R public oversized host projection refuses beside the original rooted condition\n")

work <- study()
write(work,"consumer/root.yaml",paste0(consumer,host_truth[["fatal_row_count_yaml"]]))
failed <- build(work)
stopifnot(is.null(failed$output),identical(failed$issues$condition,"row_count_failed"),
  identical(unlist(evidence(failed)$entered),c(1,0)),
  grepl('"content":"ID,VALUE\\n1,1.23\\n2,2.35\\n"',observe(failed),fixed=TRUE),
  inherits(tryCatch(failed$save(),error=identity),"yamaa_domain_error"),!dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE)
stopifnot(is.null(failed$output))
work <- study(); write(work,"raw/raw.csv","ID,VALUE\n1,bad\n")
failed <- build(work)
stopifnot(is.null(failed$output),nrow(failed$issues)>0L,identical(unlist(evidence(failed)$entered),1),
  grepl('"artifacts":[]',observe(failed),fixed=TRUE))
unlink(work,recursive=TRUE)
cat("R public ingestion and late verification failures retain evidence without accepted root output\n")

work <- study()
write(work,"consumer/root.yaml",sub("schema: ../producer/p.yaml",host_truth[["incompatible_reference"]],consumer,fixed=TRUE))
failed <- build(work)
stopifnot(is.null(evidence(failed)),is.null(failed$output),"REQ-0523" %in% failed$issues$requirement)
before <- preparation(failed)
unlink(work,recursive=TRUE)
stopifnot(identical(preparation(failed),before),any(vapply(before,function(item) identical(item$bytes,charToRaw(producer)),logical(1))))
work <- study(); write(work,"env/environment.yaml","schema_version: [\n")
failed <- build(work)
stopifnot(is.null(evidence(failed)),is.null(failed$output),
  any(vapply(preparation(failed),function(item) identical(item$bytes,charToRaw("schema_version: [\n")),logical(1))))
write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml}\n")
write(work,"consumer/root.yaml",sub("schema: ../producer/p.yaml","schema: null",consumer,fixed=TRUE))
rejected <- build(work)
stopifnot(is.null(evidence(rejected)),"REQ-0287" %in% rejected$issues$requirement)
unlink(work,recursive=TRUE)
stopifnot(any(vapply(preparation(failed),function(item) identical(item$bytes,charToRaw("schema_version: [\n")),logical(1))))
cat("R public metadata and early raw rejections retain original evidence without fallback\n")

work <- study()
write(work,"producer/layers/base.yaml",sub("name: ID, type: int","name: ID, type: str",
  sub("ID: int","ID: str",producer,fixed=TRUE),fixed=TRUE))
extra <- paste0(vapply(0:58,function(index) paste0("  - {name: EXTRA",index,
  host_truth[["extra_null_column_suffix"]]),character(1)),collapse="")
names <- paste(c("ID","VALUE",paste0("EXTRA",0:58)),collapse=", ")
text <- sub("output: {path:",paste0(extra,"output: {path:"),consumer,fixed=TRUE)
text <- sub("name: ID, type: int","name: ID, type: str",text,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",paste0("columns: [",names,"], decimals: 2"),text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
# Three public-copy passes over 23,000 rows and 63 owned columns visit
# 4,347,000 cells, exceeding the independent 4,194,304-cell allowance.
write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0(1:23000,",1.234\n",collapse="")))
result <- build(work)
refusal <- function(result) reply(.Call(private("wrap__domain_projection_failure"),handle(result)))
stopifnot(isTRUE(evidence(result)$accepted),is.null(result$output),
  identical(unlist(evidence(result)$entered),c(1,0)),
  identical(refusal(result),"table output exceeds byte limit"),
  grepl('"code":"output_projection"',result$issues$context,fixed=TRUE),
  grepl('"outcome":"success"',observe(result),fixed=TRUE),
  !dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE); invisible(gc())
stopifnot(identical(refusal(result),"table output exceeds byte limit"),isTRUE(evidence(result)$accepted),
  inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
cat("R public copy quota preserves accepted whole attempt and typed original refusal without output or Save\n")

# Text stays below the execution allowance, while IPC offsets and numeric
# columns take the public copy beyond its 8 MiB byte cap.
work <- study()
extra <- paste0(host_truth[["large_payload_column"]],
  strrep("x",2048),'"}}\n')
text <- sub("output: {path:",paste0(extra,"output: {path:"),consumer,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",host_truth[["payload_projection"]],text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0(1:4095,",1.234\n",collapse="")))
result <- build(work)
refusal <- function(result) reply(.Call(private("wrap__domain_projection_failure"),handle(result)))
stopifnot(isTRUE(evidence(result)$accepted),is.null(result$output),
  identical(unlist(evidence(result)$entered),c(1,0)),
  identical(refusal(result),"table output exceeds byte limit"),
  grepl('"code":"output_projection"',result$issues$context,fixed=TRUE),
  grepl('"outcome":"success"',observe(result),fixed=TRUE),
  !dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE); invisible(gc())
stopifnot(identical(refusal(result),"table output exceeds byte limit"),isTRUE(evidence(result)$accepted),
  inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"))
cat("R public byte copy quota preserves accepted whole attempt and typed original refusal without output or Save\n")

work <- study();dir.create(file.path(work,"generated"));dir.create(file.path(work,"generated/root.csv"))
result <- build(work)
stopifnot(identical(result$save(),FALSE),isTRUE(evidence(result)$accepted),
  identical(result$output$VALUE,c(2.46,4.70)),!file.exists(file.path(work,"generated/producer.csv")),
  grepl('"entered":[]',observe(result),fixed=TRUE))
unlink(file.path(work,"generated/root.csv"),recursive=TRUE)
stopifnot(isTRUE(result$save()),nrow(result$issues)==0L)
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
    list(0L,function(encoded) {events <<- c(events,"invoke");invoke(encoded)})
  }
  result <- with_caps(versions,binding,build(work));before <- observe(result);built <- evidence(result);calls <- events
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
  stopifnot(identical(result$save(),FALSE),identical(evidence(result),built),identical(events,calls),
    identical(result$output$VALUE,c(2.46,4.70)),"REQ-0764" %in% result$issues$requirement,
    identical(readBin(target,"raw",n=100),charToRaw(if(failed=="producer") "previous producer bytes" else "previous consumer bytes")),
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),charToRaw(if(failed=="producer") "previous producer bytes" else host_truth[["produced_csv"]])),
    grepl(if(failed=="producer") '"entered":[{"node":1,"published":false}]' else '"entered":[{"node":1,"published":true},{"node":0,"published":false}]',observe(result),fixed=TRUE),
    length(list.files(dirname(target),pattern="[.]stage$",all.files=TRUE))==0L)
  Sys.chmod(readonly,if(.Platform$OS.type=="windows") "0666" else "0700")
  stopifnot(isTRUE(result$save()),nrow(result$issues)==0L,identical(events,calls),identical(evidence(result),built),
    identical(sub('"publication":.+?,"report_version":','"report_version":',observe(result),perl=TRUE),before))
  TRUE
}
qualified <- vapply(c("producer","consumer"),save_failure,logical(1))
if(all(qualified)) cat("R actual producer/consumer publication failures preserve original Build, actual prefix and earlier writes\n")

if(.Platform$OS.type=="windows") {
  work <- study()
  dir.create(file.path(work,"generated"))
  write(work,"consumer/root.yaml",sub("../generated/root.csv","../generated/PRODUCER.csv",consumer,fixed=TRUE))
  result <- build(work)
  stopifnot(!is.null(result$output),isTRUE(evidence(result)$accepted),identical(result$save(),FALSE),
    length(list.files(file.path(work,"generated"),all.files=TRUE,no..=TRUE))==0L,
    grepl('"entered":[]',observe(result),fixed=TRUE))
  unlink(work,recursive=TRUE)
  cat("R absent Windows case-alias targets refuse every write\n")
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
    stopifnot(!is.null(result$output),isTRUE(evidence(result)$accepted))
    if(name=="PRODUCER.csv" && sensitive) {
      stopifnot(isTRUE(result$save()),
        identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
        identical(readBin(file.path(work,"generated/PRODUCER.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])))
      unlink(list.files(file.path(work,"generated"),full.names=TRUE))
    } else stopifnot(identical(result$save(),FALSE),
      grepl('"entered":[]',observe(result),fixed=TRUE),
      length(list.files(file.path(work,"generated"),all.files=TRUE,no..=TRUE))==0L)
  }
  unlink(work,recursive=TRUE)
  cat("R macOS parent case policy and unproven Unicode names refuse before publication\n")
}

# Environment absence is a request fact, even beside malformed environment YAML.
# Both Check and Build use the producer closure without activation authority.
work <- study()
write(work,"producer/layers/base.yaml",sub(host_truth[["identity_derivation"]],"RAW.VALUE",producer,fixed=TRUE))
write(work,"env/environment.yaml","[")
dir.create(file.path(work,"generated"))
poison <- function(...) stop("absent environment entered host activation")
result <- with_caps(poison,poison,{
  built <- yamaa_domain(file.path(work,"consumer/root.yaml"))
  stopifnot(nrow(built$issues)==0L,identical(built$output$VALUE,c(2.46,4.70)),
    grepl('"environment":{"lock_source":null,"source":null}',observe(built),fixed=TRUE),
    grepl('"lock":"not_requested"',observe(built),fixed=TRUE),
    !any(vapply(evidence(built)$sources,function(item) endsWith(rawToChar(item$name),"environment.yaml"),logical(1))))
  unlink(file.path(work,"raw/raw.csv"))
  stopifnot(nrow(yamaa_check(file.path(work,"consumer/root.yaml"))$issues)==0L)
  unlink(file.path(work,fixture_strings(host_truth[["retained_specification_files"]])))
  for(index in 1:2) stopifnot(isTRUE(built$save()),
    identical(readBin(file.path(work,"generated/producer.csv"),"raw",n=100),fixture_bytes(host_truth[["produced_csv"]])),
    identical(readBin(file.path(work,"generated/root.csv"),"raw",n=100),fixture_bytes(host_truth[["consumed_csv"]])))
  built
})
unlink(work,recursive=TRUE)
cat("R absent environment graphs capture no environment and repeat Save from held bytes\n")
