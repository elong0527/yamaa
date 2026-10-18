# Runtime qualification uses base R and the installed native facade only.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("report-native-only-")
dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
root <- system.file("specification-original",package="yamaa",mustWork=TRUE)
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
# Private installed-run evidence: the run's complete observations as JSON text.
observations <- function(result) yamaa:::.run_observations(result)
json_text <- function(value) {
  characters <- strsplit(enc2utf8(value),"",fixed=TRUE)[[1L]]
  escaped <- vapply(characters,function(character) {
    code <- utf8ToInt(character)
    switch(as.character(code),"8"="\\b","9"="\\t","10"="\\n","12"="\\f","13"="\\r",
      "34"="\\\"","92"="\\\\",if(code<32L) sprintf("\\u%04x",code) else character)
  },character(1L),USE.NAMES=FALSE)
  paste0('"',paste0(escaped,collapse=""),'"')
}
# One run report replaces the verification and warning logs: fourteen
# columns, with COUNT and TOTAL int and every other column str (REQ-0392).
report_columns <- c("ARTIFACT","KIND","SPEC_PATH","NAME","ID","TARGET","RULE","SEVERITY",
  "OUTCOME","CONDITION","COUNT","TOTAL","KEYS","DETAILS")
# An authored report CSV as R holds the run's report view: missing cells NA,
# COUNT and TOTAL as double.
report_view <- function(text) {
  view <- read.csv(text=text,colClasses="character",na.strings="",check.names=FALSE,encoding="UTF-8")
  for(column in c("COUNT","TOTAL")) view[[column]] <- as.double(view[[column]])
  view
}
report_csv <- function(rows) charToRaw(enc2utf8(paste0(paste(c(paste(report_columns,collapse=","),rows),collapse="\n"),"\n")))
check_report_view <- function(actual,expected) {
  stopifnot(is.data.frame(actual),identical(names(actual),report_columns),
    is.double(actual$COUNT),is.double(actual$TOTAL),
    all(vapply(actual[setdiff(report_columns,c("COUNT","TOTAL"))],is.character,logical(1))),
    identical(as.list(actual),as.list(expected)))
}

case_name <- "schema-verification-log"
directory <- tempfile("public-native-report-");dir.create(directory)
stopifnot(file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
case <- file.path(directory,case_name)
entry <- file.path(case,"spec.yaml")
built <- yamaa_spec(entry)$run()
expected <- rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json"))))
expected <- sub('"runtime":"python"','"runtime":"r"',expected,fixed=TRUE)
expected <- sub("fixture-runtime",as.character(getRversion()),expected,fixed=TRUE)
expected <- sub("fixture-engine",yamaa:::.core_version(),expected,fixed=TRUE)
expected <- sub(paste0('"example":"',case_name,'"'),'"example":"domain"',expected,fixed=TRUE)
entry_field <- paste0('"specification":',json_text(entry))
expected <- gsub('"specification":"spec.yaml"',entry_field,expected,fixed=TRUE)
unsaved <- sub('^\\{"artifacts":.*,"backend":','{"artifacts":[],"backend":',expected)
stopifnot(identical(observations(built),unsaved),nrow(built$issues)==0L)
# The benchmark's independently authored report and output are the truth.
names <- c("adsl-checks.csv","adsl.csv")
truth <- report_view(rawToChar(rawfile(file.path(case,"expected",names[[1L]]))))
check_report_view(built$report,truth)
output <- read.csv(file.path(case,"expected",names[[2L]]),colClasses="character",na.strings="",check.names=FALSE)
output$AGE <- as.double(output$AGE)
stopifnot(identical(as.list(built$output),as.list(output)))
for(name in names) stopifnot(!file.exists(file.path(case,name)))
# Views are copies: changing one leaves the run's held report unchanged.
view <- built$report
view$OUTCOME[] <- "changed"
stopifnot(identical(built$report$OUTCOME[[1L]],"succeeded"))
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
invisible(gc())
for(attempt in seq_len(2L)) {
  stopifnot(is.null(built$save()),identical(observations(built),expected))
  for(name in names) stopifnot(identical(rawfile(file.path(case,name)),rawfile(file.path(case,"expected",name))))
}
report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
if(nzchar(report_directory)) {
  observed <- observations(built)
  stopifnot(identical(observed,expected))
  observed <- sub('"example":"domain"',paste0('"example":"',case_name,'"'),observed,fixed=TRUE)
  observed <- gsub(entry_field,'"specification":"spec.yaml"',observed,fixed=TRUE)
  dir.create(report_directory,recursive=TRUE,showWarnings=FALSE)
  writeBin(charToRaw(observed),file.path(report_directory,paste0(case_name,".r.rust.json")))
}
held <- built$report
rm(built);invisible(gc())
stopifnot(nrow(held)==9L,identical(held$TOTAL[held$KIND=="check"],truth$TOTAL[truth$KIND=="check"]))
cat(case_name,"public domain complete Rust run report and successful save observations passed\n")

# A failed run keeps its report: the checks it evaluated, up to the violated
# error, and no column rows (REQ-0393). Save writes only the report file, for
# a failed run too (REQ-1173), and then signals the run's issues.
entry <- file.path(case,"spec.yaml")
source <- rawToChar(rawfile(file.path(root,"cases",case_name,"spec.yaml")))
writeBin(charToRaw(sub("severity: warning","severity: error",source,fixed=TRUE)),entry)
stopifnot(file.copy(file.path(root,"cases",case_name,"input","dm.csv"),file.path(case,"input","dm.csv")))
failed <- yamaa_spec(entry)$run()
fatal <- c(
  "adsl.csv,run,output,,,,,,failed,range_failed,,,,{}",
  'adsl.csv,input,input.DM,DM,,,,,,,3,,,"{""bytes"":66,""path"":""input/dm.csv"",""profile"":""csv""}"',
  "adsl.csv,rows,base,DM,,,,,,,3,3,,{}",
  "adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,REQ-0375,error,held,,0,3,,{}",
  'adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,REQ-0377,error,violated,range_failed,1,3,"[{""USUBJID"":""S-2""}]","{""column"":""AGE""}"')
stopifnot(is.null(failed$output),identical(failed$issues$condition,"range_failed"))
check_report_view(failed$report,report_view(rawToChar(report_csv(fatal))))
issues <- failed$issues
for(name in names) writeBin(charToRaw("sentinel"),file.path(case,name))
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
for(attempt in seq_len(2L)) {
  failure <- tryCatch(failed$save(),error=identity)
  stopifnot(inherits(failure,"yamaa_error"),identical(conditionMessage(failure),"cannot save a failed run; see issues"),
    identical(failure$issues,issues),identical(failed$issues,issues),
    identical(rawfile(file.path(case,names[[1L]])),report_csv(fatal)),
    identical(rawfile(file.path(case,names[[2L]])),charToRaw("sentinel")))
}
cat("failed run report, original issues and repeated explicit failed Save passed\n")

# A violated check lists every offending key, beyond the issue sample, and
# its declared id; a refused save writes nothing and a retry writes it all.
entry <- file.path(case,"spec.yaml")
source <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
  "output: {path: absent/out.csv, columns: [ID], report: checks.csv}\n",
  "columns:\n  - name: ID\n    type: int\n    derivation: SRC.ID\n    verifications:\n",
  "      - range: {max: 0, severity: warning, id: '\\u00e9'}\n")
# Use a double-quoted YAML scalar so the unicode escape is decoded in Rust.
source <- sub("id: '\\u00e9'",'id: "\\u00e9"',source,fixed=TRUE)
writeBin(charToRaw(source),entry)
writeLines(c("ID",as.character(seq_len(8L))),file.path(case,"input.csv"))
keys <- paste0('[',paste0('{"ID":',seq_len(8L),'}',collapse=','),']')
warned <- c(
  "absent/out.csv,run,output,,,,,,succeeded,,8,,,{}",
  'absent/out.csv,input,input.SRC,SRC,,,,,,,8,,,"{""bytes"":19,""path"":""input.csv"",""profile"":""csv""}"',
  "absent/out.csv,rows,base,SRC,,,,,,,8,8,,{}",
  "absent/out.csv,column,columns.ID,ID,,,,,,,0,8,,{}",
  paste0("absent/out.csv,check,columns.ID.verifications[0].range,range,\u00e9,ID,REQ-0377,warning,violated,range_failed,8,8,\"",
    gsub('"','""',keys,fixed=TRUE),'","{""column"":""ID""}"'))
warn <- yamaa_spec(entry)$run()
stopifnot(nrow(warn$issues)==0L,identical(warn$report$ID[warn$report$KIND=="check"],intToUtf8(233L)),
  identical(warn$report$KEYS[warn$report$KIND=="check"],keys))
check_report_view(warn$report,report_view(rawToChar(report_csv(warned))))
writeBin(charToRaw("stale"),file.path(case,"checks.csv"))
failure <- tryCatch(warn$save(),error=identity)
stopifnot(inherits(failure,"yamaa_error"),identical(conditionMessage(failure),"nothing was saved; see issues"),
  identical(failure$issues,warn$issues),identical(rawfile(file.path(case,"checks.csv")),charToRaw("stale")))
dir.create(file.path(case,"absent"))
removed <- c(entry,file.path(case,"input.csv"))
stopifnot(unlink(removed)==0L,!any(file.exists(removed)))
stopifnot(is.null(warn$save()),nrow(warn$issues)==0L,identical(rawfile(file.path(case,"checks.csv")),report_csv(warned)),
  identical(rawfile(file.path(case,"absent/out.csv")),charToRaw(paste0("ID\n",paste0(seq_len(8L),"\n",collapse="")))))
# A run without checks still reports its run, input, rows and columns.
source <- strsplit(source,"    verifications:",fixed=TRUE)[[1L]][[1L]]
writeBin(charToRaw(source),entry)
writeLines(c("ID","1"),file.path(case,"input.csv"))
unchecked <- c(
  "absent/out.csv,run,output,,,,,,succeeded,,1,,,{}",
  'absent/out.csv,input,input.SRC,SRC,,,,,,,1,,,"{""bytes"":5,""path"":""input.csv"",""profile"":""csv""}"',
  "absent/out.csv,rows,base,SRC,,,,,,,1,1,,{}",
  "absent/out.csv,column,columns.ID,ID,,,,,,,0,1,,{}")
empty <- yamaa_spec(entry)$run()
check_report_view(empty$report,report_view(rawToChar(report_csv(unchecked))))
stopifnot(!any(empty$report$KIND=="check"),is.null(empty$save()),
  identical(rawfile(file.path(case,"checks.csv")),report_csv(unchecked)))
unlink(directory,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
cat("full offending keys, unicode IDs, fresh owned views, reports without checks and retry passed\n")
