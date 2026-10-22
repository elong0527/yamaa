# Runtime qualification uses base R and the installed native facade only.
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
restore_path <- without_python()
library(yamaa)
root <- normalizePath("specification-original",winslash="/",mustWork=TRUE)
# The paired verification-logs truth is JSON, and jsonlite, which reads it, is
# a suggested package: where it is not installed, its paired outcomes are not
# compared, and the rest is.
truth <- if (requireNamespace("jsonlite", quietly = TRUE)) {
  source(file.path(host_fixture_root,"reader.R"),local=TRUE)
  read_host_fixture(host_fixture_root,"verification-logs")
}
if (is.null(truth)) message("Skipped the paired outcomes of run_report.R: jsonlite, a suggested package, is not installed")
# The report counts input bytes, so inputs end lines with LF on every
# platform: writeLines would write CRLF on Windows.
write_lf <- function(lines,path) writeBin(charToRaw(paste0(lines,"\n",collapse="")),path)
# Private installed-run evidence: the run's complete observations as JSON text.
observations <- function(result) yamaa:::.run_observations(result)
# One run report replaces the verification and warning logs: fourteen
# columns, with COUNT and TOTAL int and every other column str (CHECK-24).
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
check_report_view <- function(actual,expected,label) {
  stopifnot_case(label,"the report is a data frame"=is.data.frame(actual),
    "COUNT and TOTAL are double"=is.double(actual$COUNT) && is.double(actual$TOTAL),
    "every other column is character"=
      all(vapply(actual[setdiff(report_columns,c("COUNT","TOTAL"))],is.character,logical(1))))
  expect_identical(names(actual),report_columns,paste(label,"has the report columns"))
  expect_identical(as.list(actual),as.list(expected),paste(label,"is the authored report"))
}

case_name <- if (is.null(truth)) "schema-verification-log" else truth$case_name
directory <- tempfile("public-native-report-");dir.create(directory)
stopifnot("the case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
case <- file.path(directory,case_name)
entry <- file.path(case,"spec.yaml")
built <- yamaa_spec(entry)$run()
expected <- r_report_text(rawToChar(rawfile(file.path(root,"expected",paste0(case_name,".json")))),case_name,entry)
entry_field <- paste0('"specification":',json_text(entry))
unsaved <- unsaved_report_text(expected)
expect_identical(observations(built),unsaved,"the unsaved run's observations")
stopifnot("the run has no issues"=nrow(built$issues)==0L)
# The benchmark's independently authored report and output are the truth.
names <- c("adsl-checks.csv","adsl.csv")
report <- report_view(rawToChar(rawfile(file.path(case,"expected",names[[1L]]))))
check_report_view(built$report,report,"the run's report")
output <- read.csv(file.path(case,"expected",names[[2L]]),colClasses="character",na.strings="",check.names=FALSE)
output$AGE <- as.double(output$AGE)
expect_identical(as.list(built$output),as.list(output),"the run's output")
for(name in names) stopifnot_case(name,"nothing is written before save"=!file.exists(file.path(case,name)))
# Views are copies: changing one leaves the run's held report unchanged.
view <- built$report
view$OUTCOME[] <- "changed"
expect_identical(built$report$OUTCOME[[1L]],"succeeded","a changed view leaves the held report")
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot("the sources are removed"=unlink(removed)==0L && !any(file.exists(removed)))
invisible(gc())
for(attempt in seq_len(2L)) {
  label <- sprintf("save %d",attempt)
  # save() returns the paths it wrote, the report before the output (D38).
  expect_identical(built$save(),normalizePath(file.path(case,names),winslash="/"),
    paste(label,"returns the report, then the output"))
  expect_identical(observations(built),expected,paste(label,"records the saved artifacts"))
  for(name in names) expect_identical(rawfile(file.path(case,name)),rawfile(file.path(case,"expected",name)),
    paste(label,"writes the authored",name))
}
report_directory <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
if(nzchar(report_directory)) {
  observed <- observations(built)
  expect_identical(observed,expected,"the observations written to the report directory")
  observed <- sub('"example":"domain"',paste0('"example":"',case_name,'"'),observed,fixed=TRUE)
  observed <- gsub(entry_field,'"specification":"spec.yaml"',observed,fixed=TRUE)
  dir.create(report_directory,recursive=TRUE,showWarnings=FALSE)
  writeBin(charToRaw(observed),file.path(report_directory,paste0(case_name,".r.rust.json")))
}
held <- built$report
rm(built);invisible(gc())
# One row each for the run, its input, its rows, two columns and three checks.
expect_identical(nrow(held),8L,"the held report outlives its run")
expect_identical(held$TOTAL[held$KIND=="check"],report$TOTAL[report$KIND=="check"],"the checks' evaluated counts")
if (!is.null(truth)) expect_identical(held$TOTAL[held$KIND=="check"],as.double(fixture_strings(truth$evaluated_counts)),
  "the checks' evaluated counts are the paired truth")
cat(case_name,"public domain complete Rust run report and successful save observations passed\n")

# A failed run keeps its report: the checks it evaluated, up to the violated
# error, and no column rows (CHECK-23). Save writes only the report file, for
# a failed run too (CHECK-22), and then signals the run's issues.
entry <- file.path(case,"spec.yaml")
source <- rawToChar(rawfile(file.path(root,"cases",case_name,"spec.yaml")))
writeBin(charToRaw(sub("severity: warning","severity: error",source,fixed=TRUE)),entry)
stopifnot("the input is restored"=file.copy(file.path(root,"cases",case_name,"input","dm.csv"),file.path(case,"input","dm.csv")))
failed <- yamaa_spec(entry)$run()
fatal <- c(
  "adsl.csv,run,output,,,,,,failed,range_failed,,,,{}",
  'adsl.csv,input,input.DM,DM,,,,,,,3,,,"{""bytes"":66,""path"":""input/dm.csv"",""profile"":""csv""}"',
  "adsl.csv,rows,base,DM,,,,,,,3,3,,{}",
  "adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,CHECK-5,error,held,,0,3,,{}",
  'adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,CHECK-7,error,violated,range_failed,1,3,"[{""USUBJID"":""S-2""}]","{""column"":""AGE""}"')
stopifnot("a failed run has no output"=is.null(failed$output))
expect_identical(failed$issues$condition,"range_failed","the failed run's one issue")
check_report_view(failed$report,report_view(rawToChar(report_csv(fatal))),"the failed run's report")
if (!is.null(truth)) expect_identical(failed$report$OUTCOME[failed$report$KIND=="check"],
  fixture_strings(truth$fatal_outcomes),"the failed run's check outcomes are the paired truth")
issues <- failed$issues
for(name in names) writeBin(charToRaw("sentinel"),file.path(case,name))
removed <- c(entry,file.path(case,"input","dm.csv"))
stopifnot("the sources are removed"=unlink(removed)==0L && !any(file.exists(removed)))
for(attempt in seq_len(2L)) {
  failure <- tryCatch(failed$save(),error=identity)
  label <- sprintf("failed save %d",attempt)
  stopifnot_case(label,"the save signals yamaa_error with D39's text"=
    inherits(failure,"yamaa_error") && save_refusal_text(failure,issues))
  expect_identical(failure$issues,issues,paste(label,"carries the run's issues"))
  expect_identical(failed$issues,issues,paste(label,"leaves the run's issues"))
  expect_identical(failure$paths,normalizePath(file.path(case,names[[1L]]),winslash="/"),
    paste(label,"writes only the report"))
  expect_identical(rawfile(file.path(case,names[[1L]])),report_csv(fatal),paste(label,"writes the failed report"))
  expect_identical(rawfile(file.path(case,names[[2L]])),charToRaw("sentinel"),paste(label,"leaves the stale output"))
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
write_lf(c("ID",as.character(seq_len(8L))),file.path(case,"input.csv"))
keys <- paste0('[',paste0('{"ID":',seq_len(8L),'}',collapse=','),']')
warned <- c(
  "absent/out.csv,run,output,,,,,,succeeded,,8,,,{}",
  'absent/out.csv,input,input.SRC,SRC,,,,,,,8,,,"{""bytes"":19,""path"":""input.csv"",""profile"":""csv""}"',
  "absent/out.csv,rows,base,SRC,,,,,,,8,8,,{}",
  "absent/out.csv,column,columns.ID,ID,,,,,,,0,8,,{}",
  paste0("absent/out.csv,check,columns.ID.verifications[0].range,range,\u00e9,ID,CHECK-7,warning,violated,range_failed,8,8,\"",
    gsub('"','""',keys,fixed=TRUE),'","{""column"":""ID""}"'))
warn <- yamaa_spec(entry)$run()
stopifnot("a warning leaves no issues"=nrow(warn$issues)==0L)
expect_identical(warn$report$ID[warn$report$KIND=="check"],intToUtf8(233L),"the check's unicode id")
expect_identical(warn$report$KEYS[warn$report$KIND=="check"],keys,"the check lists every offending key")
check_report_view(warn$report,report_view(rawToChar(report_csv(warned))),"the warned run's report")
writeBin(charToRaw("stale"),file.path(case,"checks.csv"))
failure <- tryCatch(warn$save(),error=identity)
stopifnot("a save into a missing directory signals yamaa_error with D39's text"=
    inherits(failure,"yamaa_error") && save_refusal_text(failure,failure$issues),
  "the text names the dataset"=startsWith(conditionMessage(failure),"cannot save TEST: "))
expect_identical(failure$paths,character(),"a refused save writes nothing")
expect_identical(failure$issues,warn$issues,"a refused save carries the run's issues")
expect_identical(rawfile(file.path(case,"checks.csv")),charToRaw("stale"),"a refused save leaves the stale report")
dir.create(file.path(case,"absent"))
removed <- c(entry,file.path(case,"input.csv"))
stopifnot("the sources are removed"=unlink(removed)==0L && !any(file.exists(removed)))
expect_identical(warn$save(),normalizePath(file.path(case,c("checks.csv","absent/out.csv")),winslash="/"),
  "the retry writes the report, then the output")
stopifnot("the retry leaves no issues"=nrow(warn$issues)==0L)
expect_identical(rawfile(file.path(case,"checks.csv")),report_csv(warned),"the retry writes the report")
expect_identical(rawfile(file.path(case,"absent/out.csv")),charToRaw(paste0("ID\n",paste0(seq_len(8L),"\n",collapse=""))),
  "the retry writes the output")
# A run without checks still reports its run, input, rows and columns.
source <- strsplit(source,"    verifications:",fixed=TRUE)[[1L]][[1L]]
writeBin(charToRaw(source),entry)
write_lf(c("ID","1"),file.path(case,"input.csv"))
unchecked <- c(
  "absent/out.csv,run,output,,,,,,succeeded,,1,,,{}",
  'absent/out.csv,input,input.SRC,SRC,,,,,,,1,,,"{""bytes"":5,""path"":""input.csv"",""profile"":""csv""}"',
  "absent/out.csv,rows,base,SRC,,,,,,,1,1,,{}",
  "absent/out.csv,column,columns.ID,ID,,,,,,,0,1,,{}")
empty <- yamaa_spec(entry)$run()
check_report_view(empty$report,report_view(rawToChar(report_csv(unchecked))),"an unchecked run's report")
stopifnot("an unchecked run reports no check"=!any(empty$report$KIND=="check"))
expect_identical(empty$save(),normalizePath(file.path(case,c("checks.csv","absent/out.csv")),winslash="/"),
  "an unchecked run writes its report, then its output")
expect_identical(rawfile(file.path(case,"checks.csv")),report_csv(unchecked),"an unchecked run's saved report")
unlink(directory,recursive=TRUE)
restore_path()
cat("full offending keys, unicode IDs, fresh owned views, reports without checks and retry passed\n")
