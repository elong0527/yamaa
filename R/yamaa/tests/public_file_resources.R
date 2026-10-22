# Real public filesystem controls; private cache/limits/Changed stay in typed Rust.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped public_file_resources.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
source(file.path(host_fixture_root,"reader.R"),local=TRUE)
source(file.path(host_fixture_root,"assert.R"),local=TRUE)
host_truth <- read_host_fixture(host_fixture_root,"file-resources")
root <- normalizePath("specification-original", winslash="/", mustWork=TRUE)
observations <- function(run) yamaa:::.run_observations(run)

qualify_entry_relative_programs <- function(producer) {
  work <- tempfile("study-paths-"); dir.create(work)
  on.exit(unlink(work,recursive=TRUE),add=TRUE)
  truth <- host_truth[["entry_relative"]]
  shared <- file.path(work,"shared"); dir.create(shared)
  for(name in names(truth$shared)) {
    writeBin(charToRaw(truth$shared[[name]]),file.path(shared,name))
  }
  results <- list()
  for(name in names(truth$studies)) {
    study <- file.path(work,name)
    dir.create(file.path(study,"input"),recursive=TRUE)
    dir.create(file.path(study,"output"))
    writeBin(charToRaw(truth$studies[[name]]),file.path(study,"input/data.csv"))
    entry <- file.path(study,"spec.yaml")
    program <- if(producer) "consumer" else "ordinary"
    writeLines(c('yamaa: "1.0"',"type: spec",paste0("parent: ../shared/",program,".yaml")),entry)
    writeBin(charToRaw(truth$producer_entry),file.path(study,"producer.yaml"))
    if(producer) {
      # A link only types the stored artifact: callers build and save each
      # study's producer before its consumer reads it.
      produced <- yamaa_spec(file.path(study,"producer.yaml"))$run()
      stopifnot_case(paste("the producer of study", name),"the producer has no issues"=nrow(produced$issues)==0L,
        "the producer saves"=returns_paths(produced$save()),
        "it saves the study's own rows"=
          identical(rawfile(file.path(study,"output/produced.csv")),charToRaw(truth$studies[[name]])))
    }
    label <- sprintf("study %s, %s", name, if(producer) "with a producer" else "ordinary")
    stopifnot_case(label,"the entry checks"=nrow(yamaa_spec(entry)$check()$issues)==0L)
    result <- yamaa_spec(entry)$run()
    stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L,
      "nothing is written before save"=!file.exists(file.path(study,"output/result.csv")))
    results[[name]] <- result
  }
  unlink(shared,recursive=TRUE)
  for(name in names(results)) {
    study <- file.path(work,name); unlink(file.path(study,"input"),recursive=TRUE)
    expected <- charToRaw(truth$studies[[name]])
    label <- sprintf("study %s, %s", name, if(producer) "with a producer" else "ordinary")
    stopifnot_case(label,"the held run saves"=returns_paths(results[[name]]$save()),
      "it saves the study's own rows"=identical(rawfile(file.path(study,"output/result.csv")),expected),
      "it saves the report"=file.exists(file.path(study,"output/report.csv")))
    if(producer) stopifnot_case(label,"the producer's output stays"=
      identical(rawfile(file.path(study,"output/produced.csv")),expected))
  }
}
qualify_entry_relative_programs(FALSE)
qualify_entry_relative_programs(TRUE)

restore_path <- without_python()

expected_report <- function(text,entry,artifacts=FALSE) {
  text <- r_report_text(text,"schema-lookup",entry)
  if(artifacts) text else unsaved_report_text(text)
}
lookup_report <- function(entry,artifacts=FALSE) {
  expected_report(rawToChar(rawfile(file.path(root,"expected/schema-lookup.json"))),entry,artifacts)
}
new_case <- function() {
  directory <- tempfile("public-file-owner-");dir.create(directory)
  stopifnot("the schema-lookup case is copied"=file.copy(file.path(root,"cases/schema-lookup"),directory,recursive=TRUE))
  case <- file.path(directory,"schema-lookup")
  list(directory=directory,case=case,entry=normalizePath(file.path(case,"spec.yaml"),winslash="/"))
}
paths <- unlist(host_truth[["lookup_paths"]],use.names=TRUE)
inspection <- read.delim(file.path(root,"source-inspection.tsv"),sep="\t",quote="",
  comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
capture <- read.delim(file.path(root,"source-capture.tsv"),sep="\t",quote="",
  comment.char="",colClasses="character",fileEncoding="ASCII",check.names=FALSE)
expect_identical(c(nrow(inspection),nrow(capture)),c(7L,8L),"the authored inspection and capture rows")
causes <- list(missing_first=c(DM="missing"),missing_later=c(AE="missing"),
  directory_first=c(DM="not_regular_file"),directory_later=c(AE="not_regular_file"),
  mixed=c(DM="missing",AE="not_regular_file"),reverse_mixed=c(DM="not_regular_file",AE="missing"),
  all_missing=c(DM="missing",AE="missing",MEDDRA="missing"))
fail_files <- function(case,failures) {
  for(name in names(failures)) {
    path <- file.path(case,paths[[name]])
    stopifnot_case(name,"the source is removed"=unlink(path)==0L)
    if(failures[[name]]=="not_regular_file") dir.create(path)
  }
}
failed <- function(actual,expected,case,label) {
  expect_identical(observations(actual),expected,paste(label,"has the authored report"))
  expect_identical(names(actual$issues),fixture_strings(host_truth[["issue_columns"]]),paste(label,"has the issue columns"))
  stopifnot_case(label,"a failed run has no output"=is.null(actual$output),
    "contexts are character"=is.character(actual$issues$context),
    "a file issue has no span"=all(vapply(actual$issues$source_span,is.null,logical(1))),
    "a file issue has no route"=all(vapply(actual$issues$operand_route,is.null,logical(1))))
  invisible(gc())
  for(attempt in seq_len(2L)) {
    refusal <- tryCatch(actual$save(),error=identity)
    stopifnot_case(sprintf("%s, save %d",label,attempt),"the save signals yamaa_error"=inherits(refusal,"yamaa_error"),
      "it carries the run's issues"=identical(refusal$issues,actual$issues),
      "the report stays"=identical(observations(actual),expected),
      "nothing is written"=!file.exists(file.path(case,"adsl.csv")))
  }
}

# Original capture-stage partial/cache reports remain unchanged in typed tests.
# These eight physical configurations exercise public all-source preflight.
for(i in seq_len(nrow(capture))) {
  row <- capture[i,,drop=FALSE]
  kind <- if(row$kind[[1L]]=="missing") "missing" else "directory"
  name <- paste0(kind,if(row$fail_at[[1L]]=="0") "_first" else "_later")
  owner <- new_case()
  fail_files(owner$case,causes[[name]])
  expected <- expected_report(inspection$expected[[match(name,inspection$case)]],owner$entry)
  for(attempt in seq_len(2L)) failed(yamaa_spec(owner$entry)$run(),expected,owner$case,
    sprintf("capture row %d (%s), run %d",i,name,attempt))
  unlink(owner$directory,recursive=TRUE)
}
cat("eight original physical capture configurations have complete public zero-read failures\n")

for(i in seq_len(nrow(inspection))) {
  owner <- new_case()
  failures <- causes[[inspection$case[[i]]]]
  fail_files(owner$case,failures)
  # Premature study decode would produce a different failure. Every expected
  # metadata condition and its order must survive before any byte capture.
  for(name in setdiff(names(paths),names(failures)))
    writeBin(as.raw(255L),file.path(owner$case,paths[[name]]))
  stopifnot_case(inspection$case[[i]],"a check reads no study file"=nrow(yamaa_spec(owner$entry)$check()$issues)==0L)
  expected <- expected_report(inspection$expected[[i]],owner$entry)
  for(attempt in seq_len(2L)) failed(yamaa_spec(owner$entry)$run(),expected,owner$case,
    sprintf("inspection case %s, run %d",inspection$case[[i]],attempt))
  unlink(owner$directory,recursive=TRUE)
}
cat("seven complete physical metadata failures repeated before every study read passed\n")

owner <- new_case()
spec <- yamaa_spec(owner$entry)
first <- spec$run()
expected <- lookup_report(owner$entry)
expect_identical(observations(first),expected,"the first run's report")
content <- rawfile(file.path(owner$case,paths[["DM"]]))
unlink(file.path(owner$case,paths[["DM"]]))
missing <- expected_report(inspection$expected[[match("missing_first",inspection$case)]],owner$entry)
failed(spec$run(),missing,owner$case,"a fresh run without DM")
expect_identical(observations(first),expected,"the first run keeps its report")
writeBin(content,file.path(owner$case,paths[["DM"]]))
expect_identical(observations(spec$run()),expected,"a fresh run with DM restored")
unlink(owner$directory,recursive=TRUE)
cat("fresh public runs inspect current physical files and preserve previous owners passed\n")

owner <- new_case()
spec <- yamaa_spec(owner$entry)
first <- spec$run()
expected <- lookup_report(owner$entry)
source <- file.path(owner$case,paths[["AE"]])
content <- rawToChar(rawfile(source))
old <- "MYOCARDIAL INFARCTION,FATAL,150\n"
stopifnot("the AE source holds the changed line once"=
  length(gregexpr(old,content,fixed=TRUE)[[1L]])==1L && gregexpr(old,content,fixed=TRUE)[[1L]][[1L]]>0L)
writeBin(charToRaw(sub(old,"MYOCARDIAL INFARCTION,FATAL,151\n",content,fixed=TRUE)),source)
second <- spec$run()
stopifnot("the changed run has no issues"=nrow(second$issues)==0L)
expect_identical(first$output$DTHDY,c(150,80,NA_real_,NA_real_),"the first run keeps its output")
expect_identical(second$output$DTHDY,c(151,80,NA_real_,NA_real_),"a fresh run reads the changed bytes")
expect_identical(observations(first),expected,"the first run keeps its report")
stopifnot("the first run saves"=returns_paths(first$save()))
expect_identical(rawfile(file.path(owner$case,"adsl.csv")),rawfile(file.path(owner$case,"expected/adsl.csv")),
  "the first run saves the authored CSV")
unlink(owner$directory,recursive=TRUE)
cat("changed physical bytes affect fresh runs while prior output remains owned passed\n")

owner <- new_case()
actual <- yamaa_spec(owner$entry)$run()
expect_identical(observations(actual),lookup_report(owner$entry),"the lookup run's report")
expect_identical(as.list(actual$output[c("USUBJID","DTHDY")]),
  list(USUBJID=c("01","02","03","04"),DTHDY=c(150,80,NA_real_,NA_real_)),"the lookup run's output")
held <- actual$output
golden <- rawfile(file.path(owner$case,"expected/adsl.csv"))
expected <- lookup_report(owner$entry,TRUE)
unlink(owner$entry);unlink(file.path(owner$case,"input"),recursive=TRUE);invisible(gc())
stopifnot("the held run saves after its files disappear"=returns_paths(actual$save()))
expect_identical(rawfile(file.path(owner$case,"adsl.csv")),golden,"the held run saves the authored CSV")
expect_identical(observations(actual),expected,"the saved run's report")
expect_identical(actual$output,held,"saving leaves the output")
rm(actual);invisible(gc())
expect_identical(held$DTHDY,c(150,80,NA_real_,NA_real_),"an output view outlives its run")
unlink(owner$directory,recursive=TRUE)
cat("original complete lookup owner and exact held save after files disappear passed\n")

simple <- function(input) paste0('yamaa: "1.0"\ntype: spec\ndomain: TEST\nkeys: [ID]\n',
  "input: {SRC: ",json_text(input),"}\n",
  "columns: [{name: ID, type: int, derivation: SRC.ID}]\n",
  "output: {path: result.csv, columns: [ID]}\n")
directory <- tempfile("public-local-path-");dir.create(directory)
case <- file.path(directory,"case");dir.create(case)
source <- file.path(directory,"outside.csv")
writeBin(charToRaw("ID\n7\n"),source)
entry <- file.path(case,"spec.yaml")
for(written in c("../outside.csv",normalizePath(source,winslash="/"))) {
  writeBin(charToRaw(simple(written)),entry)
  actual <- yamaa_spec(entry)$run()
  stopifnot_case(paste("a source written as",written),"the run has no issues"=nrow(actual$issues)==0L,
    "it reads the outside file"=identical(actual$output$ID,7),"the run saves"=returns_paths(actual$save()),
    "the saved CSV is exact"=identical(rawfile(file.path(case,"result.csv")),charToRaw("ID\n7\n")))
}
unlink(directory,recursive=TRUE)
cat("approved parent-relative and absolute public source paths passed\n")

directory <- tempfile("public-link-");dir.create(directory)
writeBin(charToRaw("ID\n7\n"),file.path(directory,"source.csv"))
linked <- suppressWarnings(file.symlink(file.path(directory,"source.csv"),file.path(directory,"alias.csv")))
if(linked) {
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(simple("alias.csv")),entry)
  actual <- yamaa_spec(entry)$run()
  expect_identical(as.list(actual$issues[c("phase","condition","rule","spec_paths","context","source_span","operand_route")]),
    list(phase="validation",condition="resource_path_symlink",rule="FILE-8",spec_paths=list("input.SRC.path"),
      context=host_truth[["alias_context_json"]],source_span=list(NULL),operand_route=list(NULL)),
    "a source through a symbolic link is refused")
  stopifnot("a refused run has no output"=is.null(actual$output),
    "its save signals yamaa_error"=inherits(tryCatch(actual$save(),error=identity),"yamaa_error"),
    "nothing is written"=!file.exists(file.path(directory,"result.csv")))
  cat("physical public symlink refusal retains complete native condition passed\n")
} else cat("SKIP: host cannot create physical public symlink control\n")
unlink(directory,recursive=TRUE)
restore_path()
