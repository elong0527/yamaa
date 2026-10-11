# Runtime qualification uses base R and the installed native facade only.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("log-native-only-")
dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
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
case_name <- "schema-verification-log"
directory <- tempfile("public-native-logs-");dir.create(directory)
stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
case <- file.path(directory,case_name)
entry <- file.path(case,"spec.yaml")
built <- yamaa_domain(entry)
expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
expected <- sub("fixture-runtime",as.character(getRversion()),expected,fixed=TRUE)
expected <- sub("fixture-engine",engine_info()$core_version,expected,fixed=TRUE)
expected <- sub(paste0('"example":"',case_name,'"'),'"example":"domain"',expected,fixed=TRUE)
entry_field <- paste0('"specification":',json_text(entry))
expected <- gsub('"specification":"spec.yaml"',entry_field,expected,fixed=TRUE)
unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
stopifnot(identical(rawToChar(observations(built)),unsaved),nrow(built$issues)==0L)
names <- c("adsl-checks.csv","adsl-warnings.csv","adsl.csv")
views <- list(built$verification_log,built$warning_log,built$output)
for(index in seq_along(names)) {
  truth <- read.csv(file.path(case,"expected",names[[index]]),colClasses="character",na.strings="",check.names=FALSE)
  integers <- if(index==1L)c("EVALUATED_COUNT","FAILURE_COUNT") else if(index==2L)"FAILURE_COUNT" else "AGE"
  for(column in integers) truth[[column]] <- structure(truth[[column]],class="yamaa_int64_vector")
  stopifnot(identical(as.list(views[[index]]),as.list(truth)))
  stopifnot(!file.exists(file.path(case,names[[index]])))
}
view <- built$verification_log
view$REPORT_VERSION[] <- "changed"
view <- built$warning_log
view$LOG_VERSION[] <- "changed"
unlink(c(entry,file.path(case,"input","dm.csv")))
gc()
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
held <- built$verification_log
rm(built);gc()
stopifnot(nrow(held)==4L,identical(as.character(held$EVALUATED_COUNT),c("3","3","3","1")))
cat(case_name,"public domain complete Rust run report and successful save observations passed\n")

# Fatal checks retain only the explicit diagnostic publication capability.
entry <- file.path(case,"spec.yaml")
source <- rawToChar(rawfile(file.path(root,"cases",case_name,"spec.yaml")))
writeBin(charToRaw(sub("severity: warning","severity: error",source,fixed=TRUE)),entry)
stopifnot(file.copy(file.path(root,"cases",case_name,"input","dm.csv"),file.path(case,"input","dm.csv")))
failed <- yamaa_domain(entry)
stopifnot(is.null(failed$output),is.null(failed$warning_log),nrow(failed$verification_log)==2L,
  identical(failed$verification_log$OUTCOME,c("held","violated")),identical(failed$issues$condition,"range_failed"))
issues <- failed$issues
for(name in names) writeBin(charToRaw("sentinel"),file.path(case,name))
unlink(c(entry,file.path(case,"input","dm.csv")))
for(attempt in seq_len(2L)) {
  stopifnot(inherits(tryCatch(failed$save(),error=identity),"yamaa_domain_error"),identical(failed$issues,issues))
  for(name in names[-1L]) stopifnot(identical(rawfile(file.path(case,name)),charToRaw("sentinel")))
  stopifnot(length(readLines(file.path(case,names[[1L]])))==3L)
}
cat("fatal log prefix, original issues and repeated explicit failed Save passed\n")

# Full keys exceed the five-key issue sample; host frames retain native int64.
entry <- file.path(case,"spec.yaml")
source <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
  "output: {path: absent/out.csv, columns: [ID], warning_log: warnings.csv, verification_log: checks.csv}\n",
  "columns:\n  - name: ID\n    type: int\n    derivation: SRC.ID\n    verifications:\n",
  "      - range: {max: 0, severity: warning, id: '\\u00e9'}\n")
# Use a double-quoted YAML scalar so the unicode escape is decoded in Rust.
source <- sub("id: '\\u00e9'",'id: "\\u00e9"',source,fixed=TRUE)
writeBin(charToRaw(source),entry)
writeLines(c("ID",as.character(seq_len(8L))),file.path(case,"input.csv"))
warn <- yamaa_domain(entry)
stopifnot(nrow(warn$issues)==0L,nrow(warn$warning_log)==1L,
  identical(warn$warning_log$VERIFICATION_ID,intToUtf8(233L)),
  identical(as.character(warn$warning_log$FAILURE_COUNT),"8"),
  identical(warn$warning_log$OFFENDING_KEYS,paste0('[',paste0('{"ID":',seq_len(8L),'}',collapse=','),']')))
for(name in c("checks.csv","warnings.csv")) writeBin(charToRaw("stale"),file.path(case,name))
stopifnot(identical(warn$save(),FALSE))
for(name in c("checks.csv","warnings.csv")) stopifnot(identical(rawfile(file.path(case,name)),charToRaw("stale")))
dir.create(file.path(case,"absent"))
unlink(c(entry,file.path(case,"input.csv")))
stopifnot(identical(warn$save(),TRUE),nrow(warn$issues)==0L)
source <- strsplit(source,"    verifications:",fixed=TRUE)[[1L]][[1L]]
writeBin(charToRaw(source),entry)
writeLines(c("ID","1"),file.path(case,"input.csv"))
empty <- yamaa_domain(entry)
stopifnot(nrow(empty$verification_log)==0L,nrow(empty$warning_log)==0L,
  inherits(empty$verification_log$EVALUATED_COUNT,"yamaa_int64_vector"),
  inherits(empty$warning_log$FAILURE_COUNT,"yamaa_int64_vector"),identical(empty$save(),TRUE),
  length(readLines(file.path(case,"checks.csv")))==1L,length(readLines(file.path(case,"warnings.csv")))==1L)
unlink(directory,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
cat("full warning keys, unicode IDs, fresh owned frames, typed empty logs and retry passed\n")
