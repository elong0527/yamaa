library(yamaa)
namespace <- asNamespace("yamaa")
private <- function(name) get(name,namespace)
prepare <- private(".prepare_producer_file")
build <- private(".producer_build")
report <- private(".producer_report")
status <- private(".producer_status")
observe <- private(".producer_observations")
retained <- private(".producer_retained")
artifact <- private(".producer_artifact")
reply <- private(".project_reply")
root <- system.file("project-original",package="yamaa",mustWork=TRUE)
raw <- charToRaw("ID,VALUE\n1,1.234\n2,2.345\n")
produced <- charToRaw("ID,VALUE\n1,1.23\n2,2.35\n")
consumed <- charToRaw("ID,VALUE\n1,2.46\n2,4.70\n")
# R authors its environment and study independently of the Python witness.
producer <- paste0("schema_version: '1.0'\ndomain: TEST\nbase: RAW\nkeys: [ID]\n",
  "input: {RAW: {path: ../../raw/raw.csv, types: {ID: int, VALUE: float}}}\n",
  "columns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n",
  "  - {name: VALUE, type: float, label: Value, derivation: {function: {name: id_float, args: {x: RAW.VALUE}}}}\n",
  "output: {path: ../../generated/producer.csv, columns: [ID, VALUE], decimals: 2}\n")
consumer <- paste0("schema_version: '1.0'\ndomain: TEST\nbase: FIRST\nkeys: [ID]\ninput:\n",
  "  FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}\n",
  "  SECOND: {path: ../generated/producer.csv, schema: ../producer/./p.yaml}\n",
  "intermediates: [{id: SECOND_REFERENCE, dataset: SECOND, no_match: null}]\ncolumns:\n",
  "  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}\n",
  "  - {name: FIRST_VALUE, type: float, label: First value, derivation: FIRST.VALUE}\n",
  "  - {name: SECOND_VALUE, type: float, label: Second value, derivation: SECOND_REFERENCE.VALUE}\n",
  "  - {name: VALUE, type: float, label: Value, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}\n",
  "output: {path: ../generated/root.csv, columns: [ID, VALUE], decimals: 2}\n")
definition <- paste0("function: yamaabenchmarks::project_value\ndescription: Installed numeric identity.\n",
  "params: [{name: x, type: float}]\nreturns: float\ntests:\n",
  "  - {id: normal, covers: [normal, numeric-comparison], args: {x: 7.25}, result: 7.25}\n",
  "  - {id: boundary, covers: [boundary], args: {x: 0.0}, result: 0.0}\n",
  "  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n")
write <- function(work,name,value) {
  file <- file.path(work,name); dir.create(dirname(file),recursive=TRUE,showWarnings=FALSE)
  writeBin(if(is.raw(value)) value else charToRaw(value),file)
}
study <- function() {
  work <- tempfile("installed-producer-"); dir.create(work)
  work <- normalizePath(work,winslash="/")
  write(work,"yamaa-project.yaml","version: '1.0'\n")
  write(work,"producer/layers/base.yaml",producer)
  write(work,"producer/p.yaml","schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n")
  write(work,"consumer/root.yaml",consumer)
  write(work,"raw/raw.csv",raw)
  write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml}\n")
  write(work,"env/float.yaml",definition)
  stopifnot(file.copy(file.path(root,"schema-functions/r/renv.lock"),file.path(work,"env/renv.lock")))
  work
}
prepare_study <- function(work) {
  specification <- prepare(file.path(work,"consumer/root.yaml"),file.path(work,"env/environment.yaml"))
  stopifnot(identical(status(specification),"ready")); specification
}
metadata <- list("installed-r","0.1.0","independent-producer","consumer/root.yaml",".")
fields <- lapply(metadata,private(".specification_text_bytes"),maximum=4096)
capabilities <- private(".yamaa_locked_host_capabilities")()
work <- study(); specification <- prepare_study(work)
write(work,"raw/raw.csv","ID,VALUE\n1,99.99\n")
events <- character()
verify <- function(versions,calls) {
  events <<- c(events,"lock")
  capabilities$verify(versions,calls)
}
resolve <- function(call,parameters) {
  events <<- c(events,"bind")
  resolved <- capabilities$resolve(call,parameters)
  if(resolved[[1L]]!=0L) return(resolved)
  invoke <- resolved[[2L]]
  list(0L,function(encoded) {
    x <- private(".scalar_unpack")(encoded[[1L]])
    events <<- c(events,paste0("invoke:",x))
    if(x==0) write(work,"raw/raw.csv",raw)
    invoke(encoded)
  })
}
attempts <- lapply(1:2,function(index) reply(.Call(private("wrap__producer_build"),specification,fields,verify,resolve,16777216L)))
stopifnot(identical(events,rep(c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345"),2L)),
          !dir.exists(file.path(work,"generated")))
rm(specification); unlink(work,recursive=TRUE); invisible(gc())
for(attempt in attempts) {
  facts <- retained(attempt)
  stopifnot(isTRUE(facts$accepted),identical(unlist(facts$entered),c(1,0)),
            identical(artifact(attempt,1L),produced),identical(artifact(attempt,0L),consumed))
  source <- Filter(function(source) endsWith(rawToChar(source$name),"/raw/raw.csv"),facts$sources)
  stopifnot(length(source)==1L,identical(source[[1L]]$bytes,raw))
  result <- report(attempt); before <- observe(result)
  stopifnot(identical(status(result),"complete"),identical(observe(report(attempt)),before),
    grepl('"outcome":"success"',before,fixed=TRUE),
    grepl('"value":"3ff3be76c8b43958"',before,fixed=TRUE),
    grepl('"value":"4002c28f5c28f5c3"',before,fixed=TRUE),
    grepl('"value":"3ff3ae147ae147ae"',before,fixed=TRUE),
    grepl('"value":"4002cccccccccccd"',before,fixed=TRUE),
    grepl('"value":"4003ae147ae147ae"',before,fixed=TRUE),
    grepl('"value":"4012cccccccccccd"',before,fixed=TRUE),
    grepl('"invoked":false',before,fixed=TRUE))
  bounded <- report(attempt,1L)
  stopifnot(identical(status(bounded),"report_limit"),isTRUE(retained(bounded)$accepted),identical(artifact(bounded,0L),consumed))
}
stopifnot(length(events)==12L)
cat("R installed graph union, rounded aliases, repeat, held source/report and quota witnesses passed\n")

# An original condition and interrupt are rooted native payloads, never messages
# reconstructed by the report. Ordinary installed callable resolution still runs.
for(interrupted in c(FALSE,TRUE)) {
  work <- study(); specification <- prepare_study(work)
  failure <- structure(list(message="original graph host cause",call=NULL),
    class=if(interrupted) c("producer_interrupt","interrupt","condition") else c("producer_failure","error","condition"))
  resolve_failure <- function(call,parameters) {
    resolved <- capabilities$resolve(call,parameters)
    if(resolved[[1L]]!=0L) return(resolved)
    invoke <- resolved[[2L]]
    list(0L,function(encoded) {
      x <- private(".scalar_unpack")(encoded[[1L]])
      if(x==1.234) return(list(if(interrupted) 3L else 1L,failure))
      invoke(encoded)
    })
  }
  attempt <- reply(.Call(private("wrap__producer_build"),specification,fields,capabilities$verify,resolve_failure,16777216L))
  result <- report(attempt)
  rm(specification,attempt); unlink(work,recursive=TRUE); invisible(gc())
  facts <- retained(result)
  stopifnot(!facts$accepted,identical(unlist(facts$entered),1),length(facts$failures)==1L,
    identical(facts$failures[[1L]]$facts$condition,failure),
    grepl('"artifacts":[]',observe(result),fixed=TRUE),
    identical(status(result),if(interrupted) "original_boundary" else "complete"))
  if(interrupted) stopifnot(identical(tryCatch(private(".producer_propagate_interrupt")(result),interrupt=identity),failure))
}
cat("R installed original graph condition/interrupt identity survives all other owners and files\n")

work <- study(); specification <- prepare_study(work)
failed <- build(specification,metadata,1L)
stopifnot(!retained(failed)$accepted,identical(unlist(retained(failed)$entered),1),
          is.null(artifact(failed,1L)),is.null(artifact(failed,0L)),
          grepl('"code":"output_limit"',observe(report(failed)),fixed=TRUE))
unlink(work,recursive=TRUE)
cat("R native report gate stops consumer authority before producer artifact consumption\n")

work <- study()
write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml, consumer_float: consumer.yaml}\n")
write(work,"env/consumer.yaml",sub("yamaabenchmarks::project_value","yamaabenchmarks::absent_function",definition,fixed=TRUE))
text <- sub("  - {name: VALUE,","  - {name: SUM_VALUE, type: float, label: Sum, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}\n  - {name: VALUE,",consumer,fixed=TRUE)
# Replace the final VALUE derivation, keeping the independently specified sum.
text <- sub("label: Value, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}",
  "label: Value, derivation: {function: {name: consumer_float, args: {x: SUM_VALUE}}}",text,fixed=TRUE)
write(work,"consumer/root.yaml",text); unlink(file.path(work,"raw/raw.csv"))
specification <- prepare_study(work); attempt <- build(specification,metadata)
result <- report(attempt); facts <- retained(result); before <- observe(result)
stopifnot(!facts$accepted,length(facts$entered)==0L,
  grepl('"source_reads":[]',before,fixed=TRUE),grepl('"tests":[]',before,fixed=TRUE),
  grepl('"function":"consumer_float"',before,fixed=TRUE),
  grepl('/env/consumer.yaml',before,fixed=TRUE),identical(status(result),"complete"))
unlink(work,recursive=TRUE)
cat("R consumer-only binding rejection keeps full union and zero study/execution authority\n")

work <- study()
for(branch in c("left","right")) write(work,paste0(branch,".yaml"),paste0(
  "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {LEAF: {path: generated/producer.csv, schema: producer/p.yaml}}\ncolumns:\n",
  "  - {name: ID, type: int, label: Identifier, derivation: LEAF.ID}\n",
  "  - {name: VALUE, type: float, label: Value, derivation: LEAF.VALUE}\n",
  "output: {path: generated/",branch,".csv, columns: [ID, VALUE], decimals: 2}\n"))
text <- sub("../generated/producer.csv, schema: ../producer/p.yaml","../generated/left.csv, schema: ../left.yaml",consumer,fixed=TRUE)
write(work,"consumer/root.yaml",sub("../generated/producer.csv, schema: ../producer/./p.yaml","../generated/right.csv, schema: ../right.yaml",text,fixed=TRUE))
specification <- prepare_study(work); events <- character()
attempt <- reply(.Call(private("wrap__producer_build"),specification,fields,verify,resolve,16777216L))
stopifnot(identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
  identical(unlist(retained(attempt)$entered),c(3,1,2,0)),isTRUE(retained(attempt)$accepted),
  identical(artifact(attempt,3L),produced),identical(artifact(attempt,1L),produced),
  identical(artifact(attempt,2L),produced),identical(artifact(attempt,0L),consumed),
  !dir.exists(file.path(work,"generated")))
rm(specification); unlink(work,recursive=TRUE); invisible(gc())
stopifnot(identical(status(report(attempt)),"complete"))
cat("R diamond executes the shared original producer once and retains exact serialized truth\n")

work <- study(); write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: [\n")
rejected <- prepare(file.path(work,"consumer/root.yaml"),file.path(work,"env/environment.yaml"))
stopifnot(identical(status(rejected),"environment_failure"))
sources <- private(".producer_rejected_sources")(rejected)
stopifnot(length(sources)==2L)
source <- Filter(function(source) endsWith(rawToChar(source$name),"/env/environment.yaml"),sources)
unlink(work,recursive=TRUE)
stopifnot(length(source)==1L,identical(source[[1L]]$bytes,charToRaw("schema_version: '1.0'\nlanguage: [\n")))
cat("R early raw environment rejection remains owned after file deletion\n")

work <- study(); specification <- prepare_study(work)
failure <- structure(list(message="original reentrant cause",call=NULL),class=c("producer_reentrant","error","condition"))
resolve_reentrant <- function(call,parameters) {
  resolved <- capabilities$resolve(call,parameters); invoke <- resolved[[2L]]
  list(0L,function(encoded) {
    if(private(".scalar_unpack")(encoded[[1L]])==1.234) return(list(1L,failure))
    invoke(encoded)
  })
}
rendering_attempt <- reply(.Call(private("wrap__producer_build"),specification,fields,capabilities$verify,resolve_reentrant,16777216L))
conditionMessage.producer_reentrant <- function(condition) { retained(rendering_attempt); "unreachable" }
refused <- report(rendering_attempt)
stopifnot(identical(status(refused),"original_boundary"),identical(retained(refused)$failures[[1L]]$facts$condition,failure))
rm(conditionMessage.producer_reentrant,rendering_attempt,specification); unlink(work,recursive=TRUE); invisible(gc())
stopifnot(identical(retained(refused)$failures[[1L]]$facts$condition,failure))
cat("R reentrant renderer refuses promptly and retains the original condition\n")

work <- study(); write(work,"consumer/root.yaml",paste0(consumer,"verifications: [{assert: {require: 'VALUE < 0'}}]\n"))
specification <- prepare_study(work); attempt <- build(specification,metadata); result <- report(attempt)
facts <- retained(result)
stopifnot(!facts$accepted,identical(unlist(facts$entered),c(1,0)),
  identical(artifact(result,1L),produced),is.null(artifact(result,0L)),identical(status(result),"complete"),
  grepl('"outcome":"failure"',observe(result),fixed=TRUE),
  grepl('"verifications":[{',observe(result),fixed=TRUE))
rm(specification,attempt); unlink(work,recursive=TRUE); invisible(gc())
stopifnot(!retained(result)$accepted,identical(artifact(result,1L),produced))
cat("R failed consumer keeps the earlier producer artifact solely as retained evidence\n")

work <- study(); malformed <- charToRaw("ID,VALUE\n1,not-a-float\n")
write(work,"raw/raw.csv",malformed); specification <- prepare_study(work); attempt <- build(specification,metadata)
result <- report(attempt); facts <- retained(result)
stopifnot(!facts$accepted,identical(unlist(facts$entered),1),identical(status(result),"complete"),
  grepl('"artifacts":[]',observe(result),fixed=TRUE))
source <- Filter(function(source) endsWith(rawToChar(source$name),"/raw/raw.csv"),facts$sources)
stopifnot(length(source)==1L,identical(source[[1L]]$bytes,malformed))
unlink(work,recursive=TRUE)
cat("R native ingestion refusal preserves the actual raw source and entered producer prefix\n")

work <- study()
write(work,"consumer/root.yaml",sub("../generated/producer.csv, schema: ../producer/p.yaml",
  "../generated/wrong.csv, schema: ../producer/p.yaml",consumer,fixed=TRUE))
rejected <- prepare(file.path(work,"consumer/root.yaml"),file.path(work,"env/environment.yaml"))
stopifnot(identical(status(rejected),"graph_failure")); unlink(work,recursive=TRUE)
issues <- private(".producer_rejection_issues")(rejected)
stopifnot(identical(issues$requirement,"REQ-0534"),identical(issues$spec_paths[[1L]],c("input.FIRST.path","input.FIRST.schema")),
  grepl("wrong.csv",issues$context,fixed=TRUE))
stopifnot(inherits(tryCatch(private(".producer_rejection_issues")(rejected,1L),error=identity),"error"),
          identical(private(".producer_rejection_issues")(rejected),issues))
cat("R original typed graph rejection retains the authored REQ-0534 cause after file deletion\n")

# Registry capacity is admitted before any activation crossing. Empty invalid
# paths create cheap rejection owners without opening a descriptor or schema.
work <- study(); specification <- prepare_study(work)
owners <- vector("list",4096L); capacity <- NULL
for(index in seq_along(owners)) {
  value <- tryCatch(prepare("",""),error=identity)
  if(inherits(value,"error")) { capacity <- value; break }
  owners[[index]] <- value
}
stopifnot(inherits(capacity,"error"),identical(conditionMessage(capacity),"producer handle limit"))
crossed <- FALSE
verify_never <- function(...) { crossed <<- TRUE; stop("activation before handle admission") }
blocked <- tryCatch(reply(.Call(private("wrap__producer_build"),specification,fields,verify_never,capabilities$resolve,16777216L)),error=identity)
stopifnot(inherits(blocked,"error"),identical(conditionMessage(blocked),"producer handle limit"),!crossed)
rm(owners,value); invisible(gc())
attempt <- build(specification,metadata)
stopifnot(isTRUE(retained(attempt)$accepted),identical(artifact(attempt,0L),consumed))
unlink(work,recursive=TRUE)
cat("R handle capacity refuses before activation and releases safely for a fresh accepted build\n")
