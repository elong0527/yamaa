# Shared Rust graph owners run with no Python executable available. Each
# graph node declares one run report (output.report), which replaced the
# verification and warning logs.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("graph-log-native-only-");dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"producer-logs")
project <- system.file("project-original",package="yamaa",mustWork=TRUE)
specification <- system.file("specification-original",package="yamaa",mustWork=TRUE)
# Private installed-run evidence: the run's complete observations as JSON text.
observe <- function(result) yamaa:::.run_observations(result)
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
write <- function(path,value) {
  dir.create(dirname(path),recursive=TRUE,showWarnings=FALSE)
  writeBin(if(is.raw(value))value else charToRaw(enc2utf8(value)),path)
}
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
ids <- fixture_strings(host_truth[["ids_decimal"]])
primary <- fixture_bytes(host_truth[["input_csv"]])
keys <- host_truth[["offending_keys_json"]]

# Independently authored node reports (REQ-0392, REQ-0393, REQ-1175, REQ-1176).
# Every node reads the eight primary records or its producer's held artifact
# of the same bytes, emits eight rows, and its range check (max 0) is violated
# by every row, with every offending key. A failed node has no column rows.
report_columns <- c("ARTIFACT","KIND","SPEC_PATH","NAME","ID","TARGET","RULE","SEVERITY",
  "OUTCOME","CONDITION","COUNT","TOTAL","KEYS","DETAILS")
csv_field <- function(value) if(grepl('[",\n]',value)) paste0('"',gsub('"','""',value,fixed=TRUE),'"') else value
csv_row <- function(...) paste(vapply(c(...),csv_field,character(1),USE.NAMES=FALSE),collapse=",")
report_bytes <- function(rows) charToRaw(enc2utf8(paste0(paste(c(paste(report_columns,collapse=","),rows),collapse="\n"),"\n")))
node_report <- function(name,severity="warning",diamond=FALSE) {
  artifact <- paste0("generated/",name,".csv")
  inputs <- switch(name,
    producer=list(c("SRC","input.csv","")),
    left=,right=list(c("P","generated/producer.csv","producer.yaml")),
    root=if(diamond) list(c("A","generated/left.csv","left.yaml"),c("B","generated/right.csv","right.yaml"))
      else list(c("A","generated/producer.csv","producer.yaml"),c("B","generated/producer.csv","./producer.yaml")))
  failed <- identical(severity,"error")
  identifier <- if(name=="root") intToUtf8(c(233L,119070L)) else paste0(name,"-range")
  details <- function(input) paste0('{"bytes":',length(primary),',"path":"',input[[2L]],'","profile":"csv"',
    if(nzchar(input[[3L]])) paste0(',"schema":"',input[[3L]],'"'),'}')
  report_bytes(c(
    csv_row(artifact,"run","output","","","","","",if(failed)"failed" else "succeeded",
      if(failed)"range_failed" else "",if(failed)"" else "8","","","{}"),
    vapply(inputs,function(input) csv_row(artifact,"input",paste0("input.",input[[1L]]),input[[1L]],
      "","","","","","","8","","",details(input)),character(1)),
    csv_row(artifact,"rows","base",inputs[[1L]][[1L]],"","","","","","","8","8","","{}"),
    if(!failed) csv_row(artifact,"column","columns.ID","ID","","","","","","","0","8","","{}"),
    csv_row(artifact,"check","columns.ID.verifications[0].range","range",identifier,"ID","REQ-0377",
      severity,"violated","range_failed","8","8",keys,'{"column":"ID"}')))
}
# An authored report as R holds the run's report view.
report_view <- function(bytes) {
  view <- read.csv(text=rawToChar(bytes),colClasses="character",na.strings="",check.names=FALSE,encoding="UTF-8")
  for(column in c("COUNT","TOTAL")) view[[column]] <- as.double(view[[column]])
  view
}
check_report_view <- function(actual,expected) {
  stopifnot(is.data.frame(actual),identical(names(actual),report_columns),
    is.double(actual$COUNT),is.double(actual$TOTAL),identical(as.list(actual),as.list(expected)))
}

base <- tempfile("public-producer-logs-");dir.create(base)
base <- normalizePath(base,winslash="/",mustWork=TRUE)
make <- function(label,diamond=FALSE,fatal=NULL,blocked=FALSE) {
  work <- file.path(base,label);dir.create(work)
  write(file.path(work,"yamaa-project.yaml"),"version: '1.0'\n")
  # The project lists the graph's entry and links the R environment.
  write(file.path(work,"project.yaml"),"schema_version: '1.0'\nenvironments: {r: environment.yaml}\nadam: {specs: [root.yaml]}\n")
  write(file.path(work,"environment.yaml"),"schema_version: '1.0'\nlock: renv.lock\nfunctions: {}\n")
  stopifnot(file.copy(file.path(project,"schema-functions/r/renv.lock"),file.path(work,"renv.lock")))
  write(file.path(work,"input.csv"),primary)
  names <- if(diamond)fixture_strings(host_truth[["diamond_names"]]) else c("producer","root")
  for(name in names) {
    if(name=="producer") { source <- host_truth[["raw_source_reference"]];input <- "SRC" }
    else if(name %in% c("left","right")) { source <- host_truth[["producer_reference"]];input <- "P" }
    else if(diamond) { source <- host_truth[["diamond_reference"]];input <- "A" }
    else { source <- host_truth[["aliased_producer_reference"]];input <- "A" }
    intermediate <- if(name=="root")host_truth[["root_intermediate_yaml"]] else ""
    # The root's ID reads SHARED through first_available so pruning (REQ-0638) keeps
    # the intermediate, and with it the B producer link, in the resolved graph.
    # SHARED's key is a literal: keyed by the ID field it would form a dependency
    # cycle with the ID column that reads it.
    derivation <- if(name=="root")"{first_available: {sources: [A.ID, SHARED.ID]}}" else paste0(input,".ID")
    severity <- if(identical(name,fatal))"error" else "warning"
    report <- if(blocked && name=="root")"blocked/root-checks.csv" else paste0("generated/",name,"-checks.csv")
    identifier <- if(name=="root")'"\\u00e9\\U0001d11e"' else paste0(name,"-range")
    write(file.path(work,paste0(name,".yaml")),paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: ",input,"\ninput: ",source,"\n",intermediate,
      "columns:\n  - name: ID\n    type: int\n    label: Identifier\n    derivation: ",derivation,"\n",
      "    verifications: [{range: {max: 0, severity: ",severity,", id: ",identifier,"}}]\n",
      "output: {path: generated/",name,".csv, columns: [ID], report: ",report,"}\n"))
  }
  # Each node publishes its report, then its primary artifact.
  paths <- unlist(lapply(names,function(name)c(
    if(blocked && name=="root")"blocked/root-checks.csv" else paste0("generated/",name,"-checks.csv"),
    paste0("generated/",name,".csv"))),use.names=FALSE)
  output <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  evidence <- if(nzchar(output))file.path(output,"producer-graph-logs/r",label) else file.path(work,"evidence")
  dir.create(evidence,recursive=TRUE)
  write(file.path(evidence,"case-directory.txt"),work)
  provenance <- Sys.getenv("YAMAA_SUPPLEMENTAL_EVIDENCE")
  if(nzchar(provenance))write(file.path(evidence,"provenance.json"),provenance)
  list(work=work,names=names,paths=paths,evidence=evidence)
}
build <- function(case,name="TEST")yamaa_project(file.path(case$work,"project.yaml"))$spec(name)$run()
remove <- function(case) {
  paths <- file.path(case$work,c("input.csv",paste0(case$names,".yaml")))
  stopifnot(unlink(paths)==0L,!any(file.exists(paths)))
}
retain <- function(case,result,phase) {
  destination <- file.path(case$evidence,phase);dir.create(destination)
  observed <- observe(result)
  stopifnot(is.character(observed),length(observed)==1L,nzchar(observed))
  write(file.path(destination,"report.json"),observed)
  for(path in case$paths)if(file.exists(file.path(case$work,path)))
    write(file.path(destination,path),rawfile(file.path(case$work,path)))
  observed
}
execution <- function(text) {
  text <- sub('"artifacts":.*,"backend":','"artifacts":[],"backend":',text)
  sub('"publication":.*,"report_version":','"report_version":',text)
}
publication <- function(text) {
  start <- regexpr('"publication":',text,fixed=TRUE)[[1L]]
  end <- regexpr(',"report_version":',text,fixed=TRUE)[[1L]]
  stopifnot(start>0L,end>start)
  substr(text,start,end-1L)
}
count <- function(text,pattern) {
  matches <- gregexpr(pattern,text,fixed=TRUE)[[1L]]
  if(matches[[1L]]<0L)0L else length(matches)
}
# R exchanges int as double: an output view holding the int64 extreme of the
# primary input refuses rather than change it; saved bytes keep it exactly.
unheld_output <- function(result) {
  failure <- tryCatch(result$output,error=identity)
  inherits(failure,"yamaa_error") && identical(conditionMessage(failure),
    paste0("integer ",ids[[length(ids)]]," is beyond 2^53 and cannot be held exactly by an R double"))
}

for(diamond in c(FALSE,TRUE)) {
  case <- make(if(diamond)"diamond" else "aliases",diamond=diamond)
  dir.create(file.path(case$work,"generated"))
  result <- build(case)
  stopifnot(nrow(result$issues)==0L,unheld_output(result))
  check_report_view(result$report,report_view(node_report("root",diamond=diamond)))
  view <- result$report
  stopifnot(identical(view$ID[view$KIND=="check"],intToUtf8(c(233L,119070L))),
    identical(view$TOTAL[view$KIND=="check"],8),identical(view$COUNT[view$KIND=="check"],8),
    identical(view$KEYS[view$KIND=="check"],keys),!any(file.exists(file.path(case$work,case$paths))))
  before <- retain(case,result,"build")
  view$OUTCOME[] <- "mutated"
  stopifnot(!identical(result$report$OUTCOME[[1L]],"mutated"))
  remove(case);invisible(gc())
  held <- NULL
  for(attempt in 1:2) {
    stopifnot(saved(result))
    report <- retain(case,result,paste0("save-",attempt))
    stopifnot(identical(execution(report),execution(before)))
    pub <- publication(report)
    stopifnot(count(pub,'"published":true')==length(case$paths),count(pub,'"published":false')==0L)
    positions <- vapply(case$paths,function(path)regexpr(paste0('"path":"',path,'"'),pub,fixed=TRUE)[[1L]],integer(1L))
    stopifnot(all(positions>0L),all(diff(positions)>0L))
    actual <- lapply(case$paths,function(path)rawfile(file.path(case$work,path)))
    for(index in seq_along(case$paths)) {
      path <- case$paths[[index]]
      name <- sub('[-.].*','',basename(path))
      truth <- if(grepl("-checks",path,fixed=TRUE))node_report(name,diamond=diamond) else primary
      stopifnot(identical(actual[[index]],truth))
    }
    if(!is.null(held))stopifnot(identical(actual,held))
    held <- actual
  }
}
cat("actual alias/diamond complete graph reports, independent report CSVs and repeated held publication passed\n")

for(fatal in c("producer","root")) {
  case <- make(paste0("fatal-",fatal),fatal=fatal)
  result <- build(case)
  # The entry's report is missing when the graph stopped before the entry ran.
  stopifnot(is.null(result$output),identical(is.null(result$report),fatal=="producer"),
    identical(result$issues$condition,"range_failed"))
  if(fatal=="root") check_report_view(result$report,report_view(node_report("root","error")))
  issues <- result$issues
  before <- retain(case,result,"build")
  for(path in case$paths)write(file.path(case$work,path),charToRaw("earlier sentinel"))
  remove(case)
  for(attempt in 1:2) {
    stopifnot(failed_save(result),identical(result$issues,issues))
    report <- retain(case,result,paste0("save-",attempt))
    stopifnot(identical(execution(report),execution(before)))
    pub <- publication(report)
    stopifnot(grepl('"mode":"diagnostic"',pub,fixed=TRUE),count(pub,'"field":"report"')==if(fatal=="producer")1L else 2L,
      count(pub,'"field":"path"')==0L)
    for(path in case$paths) {
      name <- sub('[-.].*','',basename(path))
      truth <- if(grepl("-checks",path,fixed=TRUE) && (fatal=="root" || name=="producer"))
        node_report(name,if(name==fatal)"error" else "warning") else charToRaw("earlier sentinel")
      stopifnot(identical(rawfile(file.path(case$work,path)),truth))
    }
  }
}
cat("failed leaf/dependent original issues, actual report prefix and primary revocation passed\n")

case <- make("parents")
result <- build(case)
before <- retain(case,result,"build")
stopifnot(!dir.exists(file.path(case$work,"generated")))
remove(case)
stopifnot(refused_save(result))
report <- retain(case,result,"refused")
stopifnot(grepl('"entered":[]',publication(report),fixed=TRUE),identical(execution(report),execution(before)),
  !any(result$issues$requirement %in% "REQ-0764"))
dir.create(file.path(case$work,"generated"))
stopifnot(saved(result),nrow(result$issues)==0L)
report <- retain(case,result,"retry")
stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==length(case$paths))

for(replacement in c("generated/../generated/producer.csv","generated/root-checks.arrow")) {
  case <- make(if(grepl("arrow",replacement,fixed=TRUE))"profile" else "lexical")
  entry <- file.path(case$work,"root.yaml")
  write(entry,sub("generated/root-checks.csv",replacement,rawToChar(rawfile(entry)),fixed=TRUE))
  stopifnot(unlink(file.path(case$work,"input.csv"))==0L,!file.exists(file.path(case$work,"input.csv")))
  result <- build(case)
  stopifnot(is.null(result$output),nrow(result$issues)>0L,!any(grepl("file_not_found",result$issues$condition,fixed=TRUE)),
    failed_save(result),!any(file.exists(file.path(case$work,case$paths))))
}
cat("whole-set parent admission, no-input retry and static collision/profile refusal passed\n")

# The producer document/input bytes and its report and output truth remain the
# independently authored original benchmark; only consumer identities select
# root output names.
case <- make("original")
source <- file.path(specification,"cases/schema-verification-log")
original <- rawToChar(rawfile(file.path(source,"spec.yaml")))
write(file.path(case$work,"producer.yaml"),original)
stopifnot(file.copy(file.path(source,"input"),case$work,recursive=TRUE))
consumer <- sub("domain: ADSL","domain: ADSL\nbase: A",original,fixed=TRUE)
consumer <- gsub("adsl","root",consumer,fixed=TRUE)
consumer <- sub("  DM: input/dm.csv",host_truth[["verification_producer_aliases"]],consumer,fixed=TRUE)
consumer <- gsub("DM.","A.",consumer,fixed=TRUE)
write(file.path(case$work,"root.yaml"),consumer)
case$paths <- c("adsl-checks.csv","adsl.csv","root-checks.csv","root.csv")
producer_truth <- lapply(case$paths[1:2],function(path)rawfile(file.path(source,"expected",path)))
# The root reads the producer's held artifact through A (REQ-1176); its alias B
# is read by nothing and pruned (REQ-0638), so the root reports one input.
root_report <- gsub("adsl.csv,","root.csv,",rawToChar(producer_truth[[1L]]),fixed=TRUE)
root_report <- sub('root.csv,input,input.DM,DM,,,,,,,3,,,"{""bytes"":66,""path"":""input/dm.csv"",""profile"":""csv""}"',
  csv_row("root.csv","input","input.A","A","","","","","","","3","","",
    paste0('{"bytes":',length(producer_truth[[2L]]),',"path":"adsl.csv","profile":"csv","schema":"producer.yaml"}')),
  root_report,fixed=TRUE)
root_report <- sub("root.csv,rows,base,DM,","root.csv,rows,base,A,",root_report,fixed=TRUE)
truth <- c(producer_truth,list(charToRaw(root_report),producer_truth[[2L]]))
result <- build(case,"ADSL")
stopifnot(nrow(result$issues)==0L,count(root_report,"root.csv,input,")==1L)
check_report_view(result$report,report_view(charToRaw(root_report)))
before <- retain(case,result,"build")
removed <- file.path(case$work,c("producer.yaml","root.yaml","input/dm.csv","input.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
for(attempt in 1:2) {
  stopifnot(saved(result))
  report <- retain(case,result,paste0("save-",attempt))
  stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==length(case$paths))
  for(index in seq_along(case$paths))stopifnot(identical(rawfile(file.path(case$work,case$paths[[index]])),truth[[index]]))
}
cat("unchanged original producer report specification and exact golden bytes passed\n")

if(identical(Sys.info()[["sysname"]],"Darwin")) {
  for(target in c("generated/PRODUCER-checks.csv","GENERATED/producer-checks.csv")) {
    case <- make(if(startsWith(target,"GENERATED"))"physical-alias" else "case-alias")
    write(file.path(case$work,"generated/case-probe"),"native case witness")
    insensitive <- file.exists(file.path(case$work,"generated/CASE-PROBE"))
    stopifnot(unlink(file.path(case$work,"generated/case-probe"))==0L)
    if(insensitive) {
      entry <- file.path(case$work,"root.yaml")
      write(entry,sub("generated/root-checks.csv",target,rawToChar(rawfile(entry)),fixed=TRUE))
      if(startsWith(target,"GENERATED"))stopifnot(identical(normalizePath(file.path(case$work,"GENERATED")),normalizePath(file.path(case$work,"generated"))))
      result <- build(case)
      stopifnot(unheld_output(result),nrow(result$issues)==0L,refused_save(result),
        length(list.files(file.path(case$work,"generated")))==0L)
      report <- retain(case,result,"refused")
      stopifnot(grepl('"entered":[]',publication(report),fixed=TRUE),grepl('"field":"report"',publication(report),fixed=TRUE))
    }
  }
}

if(identical(.Platform$OS.type,"unix")) {
  case <- make("late-report",blocked=TRUE)
  dir.create(file.path(case$work,"generated"))
  write(file.path(case$work,"blocked/root-checks.csv"),"older report")
  write(file.path(case$work,"generated/root.csv"),"older primary")
  result <- build(case)
  before <- retain(case,result,"build")
  remove(case)
  blocked <- file.path(case$work,"blocked")
  Sys.chmod(blocked,"0500")
  probe <- file.path(blocked,"denied-write-probe")
  denied <- inherits(tryCatch(suppressWarnings({connection <- file(probe,"wb");close(connection);NULL}),error=identity),"error")
  if(denied) {
    stopifnot(refused_save(result))
    report <- retain(case,result,"failed")
    pub <- publication(report)
    stopifnot(identical(execution(report),execution(before)),count(pub,'"published":true')==2L,
      count(pub,'"published":false')==1L,grepl('"field":"report","node":0,"path":"blocked/root-checks.csv","published":false',pub,fixed=TRUE),
      identical(rawfile(file.path(case$work,"generated/producer.csv")),primary),
      identical(rawfile(file.path(case$work,"generated/producer-checks.csv")),node_report("producer")),
      identical(rawfile(file.path(case$work,"generated/root.csv")),charToRaw("older primary")),
      identical(rawfile(file.path(case$work,"blocked/root-checks.csv")),charToRaw("older report")))
    normative <- which(result$issues$requirement %in% "REQ-0764")
    stopifnot(length(normative)==1L)
    context <- result$issues$context[[normative]]
    stopifnot(grepl('"node":0',context,fixed=TRUE),grepl('"path":"blocked/root-checks.csv"',context,fixed=TRUE),
      grepl('"entered":3',context,fixed=TRUE),grepl('"completed":2',context,fixed=TRUE))
    Sys.chmod(blocked,"0700")
    stopifnot(saved(result),nrow(result$issues)==0L,
      identical(rawfile(file.path(case$work,"blocked/root-checks.csv")),node_report("root")),
      identical(rawfile(file.path(case$work,"generated/root.csv")),primary))
    report <- retain(case,result,"retry")
    stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==length(case$paths))
    cat("actual late report IO failure, normative target issue, completed prefix and retry passed\n")
  } else {
    stopifnot(unlink(probe)==0L)
    Sys.chmod(blocked,"0700")
    write(file.path(case$evidence,"permission-control.txt"),"host cannot deny writes; failure witness skipped\n")
    cat("host permissions cannot deny this write; actual failure witness skipped\n")
  }
}
unlink(base,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
