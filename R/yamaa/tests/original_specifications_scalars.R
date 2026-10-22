# Installed original specifications, scalar family: declarative rounding,
# date operations and temporal imputation against the independent original
# outputs, and their complete static and run failures. Python reads the same
# truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_scalars.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# A static refusal: the check finds the one issue the run reports before it
# reads any study data, so the run has no output, no observations and a
# refused save.
expect_static_refusal <- function(checked, rejected, label) {
  expect_identical(rejected$issues, checked, paste(label, "is refused by the run as by the check"))
  stopifnot_case(label, "a refused run has no output" = is.null(rejected$output),
    "a static refusal has no observations" = is.null(domain_observations(rejected)),
    "its save is refused" = save_refused(rejected))
}

local({
  directory <- tempfile("public-rounding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  case_name <- "schema-round-half-away-from-zero-ties"
  stopifnot("the rounding case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry)
  input <- file.path(case,"input","adsl.csv")
  captured <- rawfile(input)
  stopifnot("the input is removed"=unlink(input)==0L && !file.exists(input),
    "the check reads no input"=nrow(check_spec(entry)$issues)==0L)
  writeBin(captured,input)
  result <- run_spec(entry)
  read_rounded <- function(path) read.csv(path,colClasses=c("character","character",rep("numeric",4L)),
    na.strings="",check.names=FALSE)
  expected <- read_rounded(file.path(case,"expected","adsl.csv"))
  expect_identical(nrow(expected),16L,"the authored rounding output has 16 rows")
  stopifnot("the rounding run has no issues"=nrow(result$issues)==0L)
  expect_identical(result$output,expected,"rounding matches the 16 committed rows")
  stopifnot("the run leaves its specification"=identical(rawfile(entry),written),
    "nothing is written before save"=!file.exists(file.path(case,"adsl.csv")),
    "the rounding run saves"=saved(result))
  expect_identical(read_rounded(file.path(case,"adsl.csv")),expected,"the saved rounding output")
  case_name <- "negative-rounding-source"
  stopifnot("the negative case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name)
  input_directory <- file.path(case,"input")
  stopifnot("the input is removed"=unlink(input_directory,recursive=TRUE)==0L && !file.exists(input_directory))
  expected_error <- host_truth[["rounding_type_error_yaml"]]
  expect_identical(rawToChar(rawfile(file.path(case,"expected","error.yaml"))),expected_error,
    "the benchmark states the shared finding")
  checked <- check_spec(file.path(case,"spec.yaml"))$issues
  rejected <- run_spec(file.path(case,"spec.yaml"))
  expect_identical(as.list(checked[c("phase","condition","rule","spec_paths","context")]),
    list(phase="validation",condition="incompatible_input_type",rule="FORMULA-16",
      spec_paths=list("columns.RNDVAL.derivation.round_half_away_from_zero.source"),
      context='{"actual":"str","expected":"numeric","source":"SEX"}'),
    "rounding nonnumeric input is refused")
  expect_static_refusal(checked,rejected,case_name)
  stopifnot("nothing is written"=!file.exists(file.path(case,"adsl.csv")))
  cat("public original declarative rounding matches 16 committed rows and rejects original nonnumeric input before study capture without Python passed\n")
})

# Copy an original case, check it without its input, run it and compare its
# typed output, which `expected_output` reads from the case's expected file,
# and its saved bytes with that file.
expect_original_output <- function(directory, case_name, output, expected_output) {
  stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
  stopifnot_case(case_name,"the input is moved aside"=file.rename(input,held),
    "the check reads no input"=nrow(check_spec(entry)$issues)==0L,
    "the input is restored"=file.rename(held,input))
  result <- run_spec(entry)
  stopifnot_case(case_name,"the run has no issues"=nrow(result$issues)==0L)
  expect_identical(result$output,expected_output(file.path(case,"expected",output)),
    paste(case_name,"matches its typed original output"))
  stopifnot_case(case_name,"the run leaves its specification"=identical(rawfile(entry),written),
    "nothing is written before save"=!file.exists(file.path(case,output)),"the run saves"=saved(result))
  expect_identical(rawfile(file.path(case,output)),rawfile(file.path(case,"expected",output)),
    paste(case_name,"saves its original bytes"))
}

local({
  directory <- tempfile("public-temporal-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("adam-adsl-duration","adam-adsl-analysis-age")) {
    expect_original_output(directory,case_name,"adsl.csv",function(path) {
      expected <- read.csv(path,na.strings="",check.names=FALSE)
      dates <- if(case_name=="adam-adsl-duration") c("STDT","ENDT") else c("BRTHDT","RANDDT")
      for(column in dates) expected[[column]] <- as.Date(expected[[column]])
      integers <- if(case_name=="adam-adsl-duration") c("DURW","DURM") else "AAGE"
      for(column in integers) expected[[column]] <- as.double(expected[[column]])
      expected
    })
  }
  cases <- list(
    list("negative-date-diff-endpoints","TIME-1"),
    list("negative-study-day-datetime","TIME-1"),
    list("negative-epoch-day-non-date","TIME-1"),
    list("negative-date-diff-units","TIME-5"))
  for(item in cases) {
    stopifnot_case(item[[1L]],"the case is copied"=file.copy(file.path(root,"cases",item[[1L]]),directory,recursive=TRUE))
    case <- file.path(directory,item[[1L]]);unlink(file.path(case,"input"),recursive=TRUE)
    # Each benchmark states its one complete finding in its expected/error.yaml,
    # in the language it is written in (#3422); its lines are the finding's.
    stated <- sort(unique(readLines(file.path(case,"expected","error.yaml"))))
    checked <- check_spec(file.path(case,"spec.yaml"))$issues
    rejected <- run_spec(file.path(case,"spec.yaml"))
    expect_identical(as.list(checked[c("phase","rule")]),list(phase="validation",rule=item[[2L]]),
      paste(item[[1L]],"is one static finding"))
    expect_identical(error_lines(checked),stated,paste(item[[1L]],"is the finding its benchmark states"))
    expect_static_refusal(checked,rejected,item[[1L]])
    stopifnot_case(item[[1L]],"no input is created"=!file.exists(file.path(case,"input")))
  }
  cat("public complete date operations match original independent outputs and four complete static failures without Python passed\n")
})

local({
  directory <- tempfile("public-imputation-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(case_name in c("schema-birth-date","sdtm-dm-age")) {
    output <- if(case_name=="schema-birth-date") "adsl.csv" else "dm.csv"
    expect_original_output(directory,case_name,output,function(path) {
      expected <- read.csv(path,colClasses="character",na.strings="",check.names=FALSE)
      dates <- if(case_name=="schema-birth-date") fixture_strings(host_truth[["partial_date_columns"]]) else "RFICDTC"
      for(column in dates) expected[[column]] <- as.Date(expected[[column]])
      if(case_name=="schema-birth-date") expected$DTM <- as.POSIXct(expected$DTM,format="%Y-%m-%dT%H:%M:%S",tz="UTC")
      column <- if(case_name=="schema-birth-date") "DOBDY" else "AGE"
      expected[[column]] <- as.double(expected[[column]])
      expected
    })
  }
  failures <- read.delim(file.path(root,"temporal-imputation-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  expect_identical(nrow(failures),8L,"the authored imputation failures")
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    label <- paste("the imputation failure",finding$case)
    stopifnot_case(label,"the case is copied"=file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    static <- finding$phase=="validation"
    if(static) unlink(file.path(case,"input"),recursive=TRUE)
    checked <- check_spec(entry)$issues
    result <- run_spec(entry);actual <- result$issues
    expect_tsv_issue(actual,finding,label,without_layer(actual$context))
    stopifnot_case(label,"a failed run has no output"=is.null(result$output),
      "only a static failure has no observations"=identical(is.null(domain_observations(result)),static),
      "its save is refused"=save_refused(result),"no CSV is written"=length(list.files(case,pattern="[.]csv$"))==0L)
    if(static) expect_identical(checked,actual,paste(label,"is found by the check"))
    else expect_identical(nrow(checked),0L,paste(label,"is not found by the check"))
  }
  cat("public temporal imputation preserves two original outputs and eight complete failures without Python passed\n")
})

restore_path()
