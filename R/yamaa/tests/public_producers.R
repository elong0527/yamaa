library(yamaa)
namespace <- asNamespace("yamaa")
private <- function(name) get(name, namespace)
root <- system.file("project-original", package="yamaa", mustWork=TRUE)
reply <- private(".domain_reply")
handle <- function(result) get("h", envir=environment(result$save), inherits=FALSE)
observe <- function(result) rawToChar(private(".domain_observations")(result))
evidence <- function(result) reply(.Call(private("wrap__domain_producer_evidence"), handle(result)))
preparation <- function(result) reply(.Call(private("wrap__domain_preparation_sources"), handle(result)))
raw <- charToRaw("ID,VALUE\n1,1.234\n2,2.345\n")
# R authors its own environment, conformance vectors and expected public values.
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
write <- function(work, name, value) {
  path <- file.path(work, name); dir.create(dirname(path), recursive=TRUE, showWarnings=FALSE)
  writeBin(if(is.raw(value)) value else charToRaw(value), path)
}
study <- function() {
  work <- tempfile("public-producer-"); dir.create(work)
  work <- normalizePath(work, winslash="/")
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
    grepl('"value":"3ff3be76c8b43958"',before,fixed=TRUE),
    grepl('"value":"4002c28f5c28f5c3"',before,fixed=TRUE),
    grepl('"value":"3ff3ae147ae147ae"',before,fixed=TRUE),
    grepl('"value":"4002cccccccccccd"',before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,1.23\\n2,2.35\\n"',before,fixed=TRUE),
    grepl('"content":"ID,VALUE\\n1,2.46\\n2,4.70\\n"',before,fixed=TRUE),
    grepl('"invoked":false',before,fixed=TRUE))
  refusal <- tryCatch(result$save(),error=identity)
  stopifnot(inherits(refusal,"yamaa_domain_error"),
    identical(conditionMessage(refusal),"producer workflow publication is not supported"),
    identical(observe(result),before),identical(result$output$VALUE,c(2.46,4.70)))
}
cat("R public aliases, actual fresh activation, held rounded ingestion and explicit publication refusal passed\n")

work <- study()
text <- sub("output: {path:","  - {name: ROOT_UNROUNDED, type: float, label: Root precision, derivation: {compute: {expr: 'VALUE + 0.004'}}}\noutput: {path:",consumer,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2","columns: [ROOT_UNROUNDED, ID, VALUE], decimals: 2",text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
result <- build(work)
stopifnot(nrow(result$issues)==0L,identical(names(result$output),c("ROOT_UNROUNDED","ID","VALUE")),
  identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)),
  grepl('"content":"ROOT_UNROUNDED,ID,VALUE\\n2.46,1,2.46\\n4.70,2,4.70\\n"',observe(result),fixed=TRUE))
copy <- result$output; copy$VALUE <- copy$VALUE*10
stopifnot(identical(result$output$VALUE,c(2.46,4.70)))
unlink(work,recursive=TRUE)
stopifnot(identical(result$output$ROOT_UNROUNDED,c(2.464,4.704)))
cat("R public unrounded root precision, projection order and copied views passed\n")

work <- study()
bridge <- paste0("schema_version: '1.0'\ndomain: TEST\nbase: FIRST\nkeys: [ID]\n",
  "input: {FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}}\ncolumns:\n",
  "  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}\n",
  "  - {name: VALUE, type: float, label: Value, derivation: FIRST.VALUE}\n",
  "output: {path: ../generated/left.csv, columns: [ID, VALUE], decimals: 2}\n")
write(work,"bridge/left.yaml",bridge)
write(work,"bridge/right.yaml",sub("generated/left.csv","generated/right.csv",sub("schema: ../producer/p.yaml","schema: ../producer/./p.yaml",bridge,fixed=TRUE),fixed=TRUE))
text <- sub("../generated/producer.csv, schema: ../producer/p.yaml","../../generated/left.csv, schema: ../../bridge/left.yaml",sub(", no_match: null","",consumer,fixed=TRUE),fixed=TRUE)
text <- sub("../generated/producer.csv, schema: ../producer/./p.yaml","../../generated/right.csv, schema: ../../bridge/right.yaml",text,fixed=TRUE)
text <- sub("path: ../generated/root.csv","path: ../../generated/root.csv",text,fixed=TRUE)
write(work,"consumer/layers/base.yaml",text)
write(work,"consumer/root.yaml","schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: ROOT\n")
events <- character()
result <- with_caps(verify,resolve,build(work))
stopifnot(nrow(result$issues)==0L,identical(result$output$VALUE,c(2.46,4.70)),
  identical(events,c("lock","bind","invoke:7.25","invoke:0","invoke:1.234","invoke:2.345")),
  identical(unlist(evidence(result)$entered),c(3,1,2,0)),!dir.exists(file.path(work,"generated")))
unlink(work,recursive=TRUE)
stopifnot(identical(result$output$VALUE,c(2.46,4.70)))
cat("R public inherited diamond executes the shared leaf once with no prospective disk input\n")

work <- study()
bad <- sub("params: [{name: x","params: [{name: z",definition,fixed=TRUE)
bad <- gsub("args: {x:","args: {z:",bad,fixed=TRUE)
bad <- sub("short-circuit-missing:x","short-circuit-missing:z",bad,fixed=TRUE)
write(work,"env/bad.yaml",bad)
write(work,"env/environment.yaml","schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {id_float: float.yaml, bad: bad.yaml}\n")
write(work,"consumer/root.yaml",sub("{compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}","{function: {name: bad, args: {z: FIRST_VALUE}}}",consumer,fixed=TRUE))
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
write(work,"consumer/root.yaml",paste0(consumer,"verifications:\n  - row_count: {min: 99}\n"))
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
write(work,"consumer/root.yaml",sub("schema: ../producer/p.yaml","schema: ../producer/p.yaml, types: {ID: str}",consumer,fixed=TRUE))
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
extra <- paste0(vapply(0:58,function(index) paste0("  - {name: EXTRA",index,
  ", type: float, label: Extra value, derivation: {literal: 1.234}}\n"),character(1)),collapse="")
names <- paste(c("ID","VALUE",paste0("EXTRA",0:58)),collapse=", ")
text <- sub("output: {path:",paste0(extra,"output: {path:"),consumer,fixed=TRUE)
text <- sub("columns: [ID, VALUE], decimals: 2",paste0("columns: [",names,"], decimals: 2"),text,fixed=TRUE)
write(work,"consumer/root.yaml",text)
write(work,"raw/raw.csv",paste0("ID,VALUE\n",paste0(1:100,",1.234\n",collapse="")))
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
