# Runtime qualification uses base R and the installed native facade only.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("report-native-only-")
dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
host_fixture_root <- if(file.exists("host-fixtures/reader.R")) "host-fixtures" else
  system.file("host-fixtures",package="yamaa",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"run-report")
root <- system.file("specification-original",package="yamaa",mustWork=TRUE)
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
observations <- get(".domain_observations",envir=asNamespace("yamaa"))
json_text <- function(value) {
  characters <- strsplit(enc2utf8(value),"",fixed=TRUE)[[1L]]
  escaped <- vapply(characters,function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code),"8"="\\b","9"="\\t","10"="\\n","12"="\\f","13"="\\r",
      "34"="\\\"","92"="\\\\",if(code<32L) sprintf("\\u%04x",code) else character)
  },character(1L),USE.NAMES=FALSE)
  paste0('"',paste0(escaped,collapse=""),'"')
}
case_name <- host_truth[["case_name"]]
directory <- tempfile("public-native-report-");dir.create(directory)
stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
case <- file.path(directory,case_name)
entry <- file.path(case,"spec.yaml")
built <- yamaa_domain(entry)
expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
expected <- sub("fixture-runtime",as.character(getRversion()),expected,fixed=TRUE)
expected <- sub("fixture-engine",yamaa:::.core_version(),expected,fixed=TRUE)
expected <- sub(paste0('"example":"',case_name,'"'),'"example":"domain"',expected,fixed=TRUE)
entry_field <- paste0('"specification":',json_text(entry))
expected <- gsub('"specification":"spec.yaml"',entry_field,expected,fixed=TRUE)
unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
stopifnot(identical(rawToChar(observations(built)),unsaved),nrow(built$issues)==0L)
# The report and the output equal their goldens; COUNT and TOTAL keep native int64.
names <- fixture_strings(host_truth[["artifact_names"]])
views <- list(built$report,built$output)
for(index in seq_along(names)) {
  truth <- read.csv(file.path(case,"expected",names[[index]]),colClasses="character",na.strings="",check.names=FALSE)
  integers <- if(index==1L) c("COUNT","TOTAL") else "AGE"
  for(column in integers) truth[[column]] <- structure(truth[[column]],class="yamaa_int64_vector")
  stopifnot(identical(as.list(views[[index]]),as.list(truth)))
  stopifnot(!file.exists(file.path(case,names[[index]])))
}
# Every access is a fresh owned copy; changing one leaves later views intact.
view <- built$report
view$OUTCOME[] <- "changed"
stopifnot(identical(built$report,views[[1L]]))
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
invisible(gc())
for(attempt in seq_len(2L)) {
  stopifnot(identical(built$save(),TRUE),identical(rawToChar(observations(built)),expected))
  for(name in names) stopifnot(identical(rawfile(file.path(case,name)),rawfile(file.path(case,"expected",name))))
}
report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
if(nzchar(report_directory)) {
  observed <- rawToChar(observations(built))
  stopifnot(identical(observed,expected))
  observed <- sub('"example":"domain"',paste0('"example":"',case_name,'"'),observed,fixed=TRUE)
  observed <- gsub(entry_field,'"specification":"spec.yaml"',observed,fixed=TRUE)
  dir.create(report_directory,recursive=TRUE,showWarnings=FALSE)
  writeBin(charToRaw(observed),file.path(report_directory,paste0(case_name,".r.rust.json")))
}
held <- built$report
rm(built);invisible(gc())
stopifnot(nrow(held)==8L,identical(as.character(held$TOTAL[held$KIND=="check"]),fixture_strings(host_truth[["check_totals"]])))
cat(case_name,"public domain complete Rust run report and successful save observations passed\n")

# A fatal check fails the run; its report keeps the reached prefix and is the
# only file a failed Save writes.
entry <- file.path(case,"spec.yaml")
source <- rawToChar(rawfile(file.path(root,"cases",case_name,"spec.yaml")))
writeBin(charToRaw(sub("severity: warning","severity: error",source,fixed=TRUE)),entry)
stopifnot(file.copy(file.path(root,"cases",case_name,"input","dm.csv"),file.path(case,"input","dm.csv")))
failed <- yamaa_domain(entry)
report <- failed$report
checks <- report$KIND=="check"
stopifnot(is.null(failed$output),identical(report$KIND,fixture_strings(host_truth[["fatal_kinds"]])),
  identical(report$OUTCOME[checks],fixture_strings(host_truth[["fatal_outcomes"]])),
  identical(report$OUTCOME[[1L]],"failed"),identical(report$CONDITION[[1L]],"range_failed"),
  is.na(report$COUNT[[1L]]),identical(failed$issues$condition,"range_failed"))
issues <- failed$issues
for(name in names) writeBin(charToRaw("sentinel"),file.path(case,name))
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
for(attempt in seq_len(2L)) {
  stopifnot(inherits(tryCatch(failed$save(),error=identity),"yamaa_domain_error"),
    identical(failed$issues,issues),identical(failed$report,report))
  stopifnot(identical(rawfile(file.path(case,names[[2L]])),charToRaw("sentinel")),
    length(readLines(file.path(case,names[[1L]])))==6L)
}
cat("fatal report prefix, original issues and repeated failed Save of the report alone passed\n")

# Full keys exceed the five-key issue sample; host frames retain native int64.
# A double-quoted YAML scalar lets Rust decode the unicode escape.
source <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
  "output: {path: absent/out.csv, columns: [ID], report: checks.csv}\n",
  "columns:\n  - name: ID\n    type: int\n    derivation: SRC.ID\n    verifications:\n",
  "      - range: {max: 0, severity: warning, id: \"\\u00e9\"}\n")
writeBin(charToRaw(source),entry)
writeLines(c("ID",as.character(seq_len(8L))),file.path(case,"input.csv"))
warn <- yamaa_domain(entry)
report <- warn$report
check <- report[nrow(report),]
stopifnot(nrow(warn$issues)==0L,identical(check$KIND,"check"),identical(check$ID,intToUtf8(233L)),
  identical(as.character(check$COUNT),"8"),identical(as.character(check$TOTAL),"8"),
  identical(check$KEYS,paste0('[',paste0('{"ID":',seq_len(8L),'}',collapse=','),']')))
writeBin(charToRaw("stale"),file.path(case,"checks.csv"))
stopifnot(identical(warn$save(),FALSE),identical(rawfile(file.path(case,"checks.csv")),charToRaw("stale")))
dir.create(file.path(case,"absent"))
removed <- c(entry,file.path(case,"input.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
stopifnot(identical(warn$save(),TRUE),nrow(warn$issues)==0L)
source <- strsplit(source,"    verifications:",fixed=TRUE)[[1L]][[1L]]
writeBin(charToRaw(source),entry)
writeLines(c("ID","1"),file.path(case,"input.csv"))
empty <- yamaa_domain(entry)
stopifnot(identical(empty$report$KIND,c("run","input","rows","column")),
  inherits(empty$report$COUNT,"yamaa_int64_vector"),inherits(empty$report$TOTAL,"yamaa_int64_vector"),
  identical(empty$save(),TRUE),length(readLines(file.path(case,"checks.csv")))==5L)
cat("full warning keys, unicode IDs, typed check-free reports and target retry passed\n")

# A report path equal to output.path, or one with an unmapped extension, is a
# validation refusal: the input is absent, so each precedes study access.
refusals <- file.path(directory,"refusals");dir.create(refusals)
entry <- file.path(refusals,"spec.yaml")
for(path in c("out.csv","checks.arrow")) {
  writeBin(charToRaw(paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
    "output: {path: out.csv, columns: [ID], report: ",path,"}\n",
    "columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n")),entry)
  collision <- path=="out.csv"
  refused <- yamaa_domain(entry)
  issues <- refused$issues
  stopifnot(identical(issues$phase,"validation"),
    identical(issues$condition,if(collision) "artifact_path_collision" else "unknown_artifact_profile"),
    identical(issues$requirement,if(collision) "REQ-0756" else "REQ-0760"),
    identical(issues$spec_paths,list(if(collision) c("output.path","output.report") else "output.report")),
    identical(issues$context,if(collision) '{"path":"out.csv"}' else '{"path":"checks.arrow","permitted":[".csv",".parquet"]}'),
    identical(yamaa_check(entry)$issues,issues),is.null(refused$report),is.null(refused$output),
    is.null(observations(refused)),inherits(tryCatch(refused$save(),error=identity),"yamaa_domain_error"),
    identical(refused$issues,issues),identical(list.files(refusals),"spec.yaml"))
}
unlink(directory,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
cat("report path collisions and unmapped report profiles refuse before study access and write nothing passed\n")
