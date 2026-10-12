# Shared Rust graph owners run with no Python executable available.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("graph-log-native-only-");dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
project <- system.file("project-original",package="yamaa",mustWork=TRUE)
specification <- system.file("specification-original",package="yamaa",mustWork=TRUE)
observe <- get(".domain_observations",envir=asNamespace("yamaa"))
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
write <- function(path,value) {
  dir.create(dirname(path),recursive=TRUE,showWarnings=FALSE)
  writeBin(if(is.raw(value))value else charToRaw(enc2utf8(value)),path)
}
ids <- c("1","2","3","4","5","6","7","9223372036854775807")
primary <- charToRaw(paste0("ID\n",paste(ids,collapse="\n"),"\n"))
keys <- paste0("[",paste0('{"ID":',ids,'}',collapse=","),"]")
vheader <- "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n"
wheader <- "LOG_VERSION,ARTIFACT,SEVERITY,CONDITION,REQUIREMENT,SPEC_PATH,VERIFICATION_ID,FAILURE_COUNT,OFFENDING_KEYS,DETAILS\n"
golden <- function(name,warning=FALSE,severity="warning") {
  # These complete range records are authored from the normative log schema.
  identifier <- if(name=="root")intToUtf8(c(233L,119070L)) else paste0(name,"-range")
  details <- '"{""column"":""ID""}"'
  row <- if(warning) paste0("1.0,generated/",name,".csv,warning,range_failed,REQ-0377,columns.ID.verifications[0].range,",identifier,',8,"',gsub('"','""',keys,fixed=TRUE),'",',details,"\n") else
    paste0("1.0,generated/",name,".csv,columns.ID.verifications[0].range,",identifier,",range,ID,REQ-0377,",severity,",violated,range_failed,8,8,",details,"\n")
  charToRaw(enc2utf8(paste0(if(warning)wheader else vheader,row)))
}
base <- tempfile("public-producer-logs-");dir.create(base)
base <- normalizePath(base,winslash="/",mustWork=TRUE)
make <- function(label,diamond=FALSE,fatal=NULL,blocked=FALSE) {
  work <- file.path(base,label);dir.create(work)
  write(file.path(work,"yamaa-project.yaml"),"version: '1.0'\n")
  write(file.path(work,"environment.yaml"),"schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {}\n")
  stopifnot(file.copy(file.path(project,"schema-functions/r/renv.lock"),file.path(work,"renv.lock")))
  write(file.path(work,"input.csv"),primary)
  names <- if(diamond)c("producer","left","right","root") else c("producer","root")
  for(name in names) {
    if(name=="producer") { source <- "{SRC: {path: input.csv, types: {ID: int}}}";input <- "SRC" }
    else if(name %in% c("left","right")) { source <- "{P: {path: generated/producer.csv, schema: producer.yaml}}";input <- "P" }
    else if(diamond) { source <- "{A: {path: generated/left.csv, schema: left.yaml}, B: {path: generated/right.csv, schema: right.yaml}}";input <- "A" }
    else { source <- "{A: {path: generated/producer.csv, schema: producer.yaml}, B: {path: generated/producer.csv, schema: ./producer.yaml}}";input <- "A" }
    intermediate <- if(name=="root")"intermediates: [{id: SHARED, dataset: B, key: [ID], no_match: null}]\n" else ""
    severity <- if(identical(name,fatal))"error" else "warning"
    warning <- if(blocked && name=="root")"blocked/root-warnings.csv" else paste0("generated/",name,"-warnings.csv")
    identifier <- if(name=="root")'"\\u00e9\\U0001d11e"' else paste0(name,"-range")
    write(file.path(work,paste0(name,".yaml")),paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: ",input,"\ninput: ",source,"\n",intermediate,
      "columns:\n  - name: ID\n    type: int\n    label: Identifier\n    derivation: ",input,".ID\n",
      "    verifications: [{range: {max: 0, severity: ",severity,", id: ",identifier,"}}]\n",
      "output: {path: generated/",name,".csv, columns: [ID], verification_log: generated/",name,"-checks.csv, warning_log: ",warning,"}\n"))
  }
  paths <- unlist(lapply(names,function(name)c(paste0("generated/",name,"-checks.csv"),
    if(blocked && name=="root")"blocked/root-warnings.csv" else paste0("generated/",name,"-warnings.csv"),paste0("generated/",name,".csv"))),use.names=FALSE)
  output <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
  evidence <- if(nzchar(output))file.path(output,"producer-graph-logs/r",label) else file.path(work,"evidence")
  dir.create(evidence,recursive=TRUE)
  write(file.path(evidence,"case-directory.txt"),work)
  provenance <- Sys.getenv("YAMAA_SUPPLEMENTAL_EVIDENCE")
  if(nzchar(provenance))write(file.path(evidence,"provenance.json"),provenance)
  list(work=work,names=names,paths=paths,evidence=evidence)
}
build <- function(case)yamaa_domain(file.path(case$work,"root.yaml"),environment=file.path(case$work,"environment.yaml"))
remove <- function(case) {
  paths <- file.path(case$work,c("input.csv",paste0(case$names,".yaml")))
  stopifnot(unlink(paths)==0L,!any(file.exists(paths)))
}
retain <- function(case,result,phase) {
  destination <- file.path(case$evidence,phase);dir.create(destination)
  observed <- observe(result)
  stopifnot(is.raw(observed),length(observed)>0L)
  write(file.path(destination,"report.json"),observed)
  for(path in case$paths)if(file.exists(file.path(case$work,path)))
    write(file.path(destination,path),rawfile(file.path(case$work,path)))
  rawToChar(observed)
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

for(diamond in c(FALSE,TRUE)) {
  case <- make(if(diamond)"diamond" else "aliases",diamond=diamond)
  dir.create(file.path(case$work,"generated"))
  result <- build(case)
  stopifnot(nrow(result$issues)==0L,identical(as.character(result$output$ID),ids))
  verification <- result$verification_log;warning <- result$warning_log
  stopifnot(identical(verification$VERIFICATION_ID,intToUtf8(c(233L,119070L))),
    identical(as.character(verification$EVALUATED_COUNT),"8"),
    identical(as.character(warning$FAILURE_COUNT),"8"),identical(warning$OFFENDING_KEYS,keys),
    inherits(verification$EVALUATED_COUNT,"yamaa_int64_vector"),inherits(warning$FAILURE_COUNT,"yamaa_int64_vector"),
    !any(file.exists(file.path(case$work,case$paths))))
  before <- retain(case,result,"build")
  verification$REPORT_VERSION[] <- "mutated"
  stopifnot(!identical(result$verification_log$REPORT_VERSION,"mutated"))
  remove(case);gc()
  held <- NULL
  for(attempt in 1:2) {
    stopifnot(isTRUE(result$save()))
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
      truth <- if(grepl("-checks",path,fixed=TRUE))golden(name) else if(grepl("-warnings",path,fixed=TRUE))golden(name,TRUE) else primary
      stopifnot(identical(actual[[index]],truth))
    }
    if(!is.null(held))stopifnot(identical(actual,held))
    held <- actual
  }
}
cat("actual alias/diamond complete graph reports, independent log CSVs and repeated held publication passed\n")

for(fatal in c("producer","root")) {
  case <- make(paste0("fatal-",fatal),fatal=fatal)
  result <- build(case)
  stopifnot(is.null(result$output),is.null(result$warning_log),identical(is.null(result$verification_log),fatal=="producer"),
    identical(result$issues$condition,"range_failed"))
  issues <- result$issues
  before <- retain(case,result,"build")
  for(path in case$paths)write(file.path(case$work,path),charToRaw("earlier sentinel"))
  remove(case)
  for(attempt in 1:2) {
    stopifnot(inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),identical(result$issues,issues))
    report <- retain(case,result,paste0("save-",attempt))
    stopifnot(identical(execution(report),execution(before)))
    pub <- publication(report)
    stopifnot(grepl('"mode":"diagnostic"',pub,fixed=TRUE),count(pub,'"field":"verification_log"')==if(fatal=="producer")1L else 2L,
      count(pub,'"field":"warning_log"')==0L)
    for(path in case$paths) {
      name <- sub('[-.].*','',basename(path))
      truth <- if(grepl("-checks",path,fixed=TRUE) && (fatal=="root" || name=="producer"))
        golden(name,severity=if(name==fatal)"error" else "warning") else charToRaw("earlier sentinel")
      stopifnot(identical(rawfile(file.path(case$work,path)),truth))
    }
  }
}
cat("failed leaf/dependent original issues, actual V prefix and primary/warning revocation passed\n")

case <- make("parents")
result <- build(case)
before <- retain(case,result,"build")
stopifnot(!dir.exists(file.path(case$work,"generated")))
remove(case)
stopifnot(identical(result$save(),FALSE))
report <- retain(case,result,"refused")
stopifnot(grepl('"entered":[]',publication(report),fixed=TRUE),identical(execution(report),execution(before)),
  !any(result$issues$requirement %in% "REQ-0764"))
dir.create(file.path(case$work,"generated"))
stopifnot(isTRUE(result$save()),nrow(result$issues)==0L)
report <- retain(case,result,"retry")
stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==6L)

for(replacement in c("generated/../generated/producer.csv","generated/root-checks.arrow")) {
  case <- make(if(grepl("arrow",replacement,fixed=TRUE))"profile" else "lexical")
  entry <- file.path(case$work,"root.yaml")
  write(entry,sub("generated/root-checks.csv",replacement,rawToChar(rawfile(entry)),fixed=TRUE))
  stopifnot(unlink(file.path(case$work,"input.csv"))==0L,!file.exists(file.path(case$work,"input.csv")))
  result <- build(case)
  stopifnot(is.null(result$output),nrow(result$issues)>0L,!any(grepl("file_not_found",result$issues$condition,fixed=TRUE)),
    inherits(tryCatch(result$save(),error=identity),"yamaa_domain_error"),!any(file.exists(file.path(case$work,case$paths))))
}
cat("whole-set parent admission, no-input retry and static collision/profile refusal passed\n")

# The producer document/input bytes and six CSV truths remain independently
# retained original fixtures; only consumer identities select root output names.
case <- make("original")
source <- file.path(specification,"cases/schema-verification-log")
original <- rawToChar(rawfile(file.path(source,"spec.yaml")))
write(file.path(case$work,"producer.yaml"),original)
stopifnot(file.copy(file.path(source,"input"),case$work,recursive=TRUE))
consumer <- sub("domain: ADSL","domain: ADSL\nbase: A",original,fixed=TRUE)
consumer <- gsub("adsl","root",consumer,fixed=TRUE)
consumer <- sub("  DM: input/dm.csv","  A: {path: adsl.csv, schema: producer.yaml}\n  B: {path: adsl.csv, schema: ./producer.yaml}",consumer,fixed=TRUE)
consumer <- gsub("DM.","A.",consumer,fixed=TRUE)
write(file.path(case$work,"root.yaml"),consumer)
case$paths <- c("adsl-checks.csv","adsl-warnings.csv","adsl.csv","root-checks.csv","root-warnings.csv","root.csv")
result <- build(case)
stopifnot(nrow(result$issues)==0L,nrow(result$verification_log)==4L,nrow(result$warning_log)==1L)
before <- retain(case,result,"build")
removed <- file.path(case$work,c("producer.yaml","root.yaml","input/dm.csv","input.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
truth <- lapply(case$paths[1:3],function(path)rawfile(file.path(source,"expected",path)))
root_truth <- lapply(truth,function(value)charToRaw(gsub("adsl.csv","root.csv",rawToChar(value),fixed=TRUE)))
truth <- c(truth,root_truth)
for(attempt in 1:2) {
  stopifnot(isTRUE(result$save()))
  report <- retain(case,result,paste0("save-",attempt))
  stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==6L)
  for(index in seq_along(case$paths))stopifnot(identical(rawfile(file.path(case$work,case$paths[[index]])),truth[[index]]))
}
cat("unchanged original producer log specification and exact golden bytes passed\n")

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
      stopifnot(!is.null(result$output),identical(result$save(),FALSE),length(list.files(file.path(case$work,"generated")))==0L)
      report <- retain(case,result,"refused")
      stopifnot(grepl('"entered":[]',publication(report),fixed=TRUE),grepl('"field":"verification_log"',publication(report),fixed=TRUE))
    }
  }
}

if(identical(.Platform$OS.type,"unix")) {
  case <- make("late-warning",blocked=TRUE)
  dir.create(file.path(case$work,"generated"))
  write(file.path(case$work,"blocked/root-warnings.csv"),"older warning")
  write(file.path(case$work,"generated/root.csv"),"older primary")
  result <- build(case)
  before <- retain(case,result,"build")
  remove(case)
  blocked <- file.path(case$work,"blocked")
  Sys.chmod(blocked,"0500")
  probe <- file.path(blocked,"denied-write-probe")
  denied <- inherits(tryCatch(suppressWarnings({connection <- file(probe,"wb");close(connection);NULL}),error=identity),"error")
  if(denied) {
    stopifnot(identical(result$save(),FALSE))
    report <- retain(case,result,"failed")
    pub <- publication(report)
    stopifnot(identical(execution(report),execution(before)),count(pub,'"published":true')==4L,
      count(pub,'"published":false')==1L,grepl('"field":"warning_log"',pub,fixed=TRUE),
      identical(rawfile(file.path(case$work,"generated/producer.csv")),primary),
      identical(rawfile(file.path(case$work,"generated/producer-checks.csv")),golden("producer")),
      identical(rawfile(file.path(case$work,"generated/producer-warnings.csv")),golden("producer",TRUE)),
      identical(rawfile(file.path(case$work,"generated/root-checks.csv")),golden("root")),
      identical(rawfile(file.path(case$work,"generated/root.csv")),charToRaw("older primary")),
      identical(rawfile(file.path(case$work,"blocked/root-warnings.csv")),charToRaw("older warning")))
    normative <- which(result$issues$requirement %in% "REQ-0764")
    stopifnot(length(normative)==1L)
    context <- result$issues$context[[normative]]
    stopifnot(grepl('"node":0',context,fixed=TRUE),grepl('"path":"blocked/root-warnings.csv"',context,fixed=TRUE),
      grepl('"entered":5',context,fixed=TRUE),grepl('"completed":4',context,fixed=TRUE))
    Sys.chmod(blocked,"0700")
    stopifnot(isTRUE(result$save()),nrow(result$issues)==0L)
    report <- retain(case,result,"retry")
    stopifnot(identical(execution(report),execution(before)),count(publication(report),'"published":true')==6L)
    cat("actual late warning IO failure, normative target issue, completed prefix and retry passed\n")
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
