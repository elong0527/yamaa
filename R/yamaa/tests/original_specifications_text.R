# Installed original specifications, text family: string case, padding,
# str_extract and str_template against independent exact outputs, complete
# findings and reached handler ledgers. Python reads the same truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_text.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# An authored TSV under specification-original, every cell text.
read_tsv <- function(name) read.delim(file.path(root,name),quote="",colClasses="character",check.names=FALSE)
# A run's handler ledger, where the run has observations and `handlers`, the
# TSV of each case's complete ledger, authors one for `name`.
check_handlers <- function(result, handlers, name) {
  report <- domain_observations(result)
  expected <- handlers$handlers[handlers$case==name]
  if(!is.null(report) && length(expected)) {
    expect_identical(length(expected),1L,paste(name,"has one authored ledger"))
    expect_ledger_twice(report,expected,name)
  }
}
# A one-issue static refusal of an original case, checked without its input;
# a `layered` finding's context is compared without the layer it names.
expect_original_refusal <- function(directory, case_name, expected, layered = FALSE) {
  stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- check_spec(file.path(case,"spec.yaml"))$issues;rejected <- run_spec(file.path(case,"spec.yaml"))
  finding <- as.list(checked[c("phase","condition","rule","spec_paths","context")])
  if(layered) finding$context <- without_layer(finding$context)
  expect_identical(finding,expected,paste(case_name,"is its one static finding"))
  expect_identical(rejected$issues,checked,paste(case_name,"is refused by the run as by the check"))
  stopifnot_case(case_name,"a refused run has no output"=is.null(rejected$output),
    "a static refusal has no observations"=is.null(domain_observations(rejected)),
    "its save is refused"=save_refused(rejected),"no input is created"=!file.exists(file.path(case,"input")))
}
# An original case that builds its benchmark's output: checked without its
# input, run, and compared with its typed and exact expected output.
expect_original_build <- function(directory, case_name, output, typed = identity, handlers = NULL) {
  stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  written <- rawfile(entry);input <- file.path(case,"input");held <- file.path(case,"held")
  stopifnot_case(case_name,"the input is moved aside"=file.rename(input,held),
    "the check reads no input"=nrow(check_spec(entry)$issues)==0L,"the input is restored"=file.rename(held,input))
  result <- run_spec(entry)
  if(!is.null(handlers)) check_handlers(result,handlers,case_name)
  expected <- typed(read.csv(file.path(case,"expected",output),colClasses="character",na.strings="",check.names=FALSE))
  stopifnot_case(case_name,"the run has no issues"=nrow(result$issues)==0L)
  expect_identical(result$output,expected,paste(case_name,"matches its typed original output"))
  stopifnot_case(case_name,"the run leaves its specification"=identical(rawfile(entry),written),
    "nothing is written before save"=!file.exists(file.path(case,output)),"the run saves"=saved(result))
  expect_identical(rawfile(file.path(case,output)),rawfile(file.path(case,"expected",output)),
    paste(case_name,"saves its original bytes"))
}
# One authored refusal row of a TSV, for a case run with its input.
expect_case_failure <- function(rejected, finding, label, checked = NULL) {
  expect_tsv_issue(rejected$issues,finding,label)
  if(!is.null(checked)) expect_identical(checked,rejected$issues,paste(label,"is found by the check"))
  stopifnot_case(label,"a refused run has no output"=is.null(rejected$output),"its save is refused"=save_refused(rejected))
}

local({
  directory <- tempfile("public-string-case-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  source <- host_truth[["unicode_case_yaml"]]
  entry <- file.path(directory,"spec.yaml");writeBin(charToRaw(source),entry)
  stopifnot("the string case checks"=nrow(check_spec(entry)$issues)==0L)
  writeBin(charToRaw("ID,TEXT\n1,FOLLOW-UP\n2,\u00df\u0130\u00e9\U0001f642AbC\n3,\n"),file.path(directory,"input.csv"))
  result <- run_spec(entry)
  expected_bytes <- fixture_bytes(host_truth[["unicode_case_csv"]])
  expected <- data.frame(ID=c("1","2","3"),UP=c("FOLLOW-UP","\u00df\u0130\u00e9\U0001f642ABC","MiXeD"),
    LOW=c("follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    SENT=c("Follow-up","\u00df\u0130\u00e9\U0001f642abc",NA_character_),
    TITLE=c("Follow-Up","\u00df\u0130\u00e9\U0001f642Abc",NA_character_),stringsAsFactors=FALSE)
  stopifnot("the string case run has no issues"=nrow(result$issues)==0L)
  expect_identical(result$output,expected,"ASCII string case leaves other characters")
  stopifnot("nothing is written before save"=!file.exists(file.path(directory,"output.csv")),
    "the string case run saves"=saved(result))
  expect_identical(rawfile(file.path(directory,"output.csv")),expected_bytes,"the saved string case output")
  source <- sub("to: upper, missing: MiXeD","to: upper",source,fixed=TRUE)
  writeBin(charToRaw(source),entry);writeBin(charToRaw("ID,TEXT\nrow1,\n"),file.path(directory,"input.csv"))
  # Without `missing`, a missing source gives missing and reports no finding,
  # as the independent text-case-reference.json records.
  answered <- run_spec(entry)
  stopifnot("a missing source reports no finding"=nrow(answered$issues)==0L)
  expect_identical(answered$output,data.frame(ID="row1",UP=NA_character_,
    LOW=NA_character_,SENT=NA_character_,TITLE=NA_character_,stringsAsFactors=FALSE),
    "a missing source gives missing")
  failures <- read_tsv("text-case-diagnostics.tsv")
  expect_identical(nrow(failures),2L,"the authored string case failures")
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,]
    stopifnot_case(finding$case,"the case is copied"=file.copy(file.path(root,"cases",finding$case),directory,recursive=TRUE))
    case <- file.path(directory,finding$case);entry <- file.path(case,"spec.yaml")
    unlink(file.path(case,"input"),recursive=TRUE)
    checked <- check_spec(entry)$issues;rejected <- run_spec(entry);actual <- rejected$issues
    expect_tsv_issue(actual,finding,finding$case,without_layer(actual$context))
    expect_identical(checked,actual,paste(finding$case,"is found by the check"))
    stopifnot_case(finding$case,"a refused run has no output"=is.null(rejected$output),
      "a static refusal has no observations"=is.null(domain_observations(rejected)),
      "its save is refused"=save_refused(rejected),"no input is created"=!file.exists(file.path(case,"input")))
  }
  case_name <- "schema-sentence-case"
  stopifnot("the sentence case is copied"=file.copy(file.path(root,"cases",case_name),directory,recursive=TRUE))
  case <- file.path(directory,case_name);entry <- file.path(case,"spec.yaml")
  checked <- check_spec(entry)$issues;result <- run_spec(entry)
  stopifnot("the sentence case checks"=nrow(checked)==0L,"the sentence case runs"=nrow(result$issues)==0L,
    "it has output"=!is.null(result$output),"nothing is written before save"=!file.exists(file.path(case,"adlb.csv")),
    "the sentence case saves"=saved(result))
  expect_identical(rawfile(file.path(case,"adlb.csv")),rawfile(file.path(case,"expected","adlb.csv")),
    "the sentence case saves its original bytes")
  cat("public ASCII string case matches independent exact outputs, original negatives and original ordered sentence artifact without Python passed\n")
})

local({
  directory <- tempfile("public-string-padding-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  cases <- file.path(root,"text-pad-cases")
  findings <- read_tsv("text-pad-findings.tsv")
  handler_truth <- read_tsv("text-pad-handlers.tsv")
  for(name in list.dirs(cases,full.names=FALSE,recursive=FALSE)) {
    label <- paste("the padding case",name)
    stopifnot_case(label,"the case is copied"=file.copy(file.path(cases,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    if(name=="width-noninteger") unlink(file.path(case,"input.csv"))
    result <- run_spec(entry)
    expected_csv <- file.path(case,"expected.csv")
    if(file.exists(expected_csv)) {
      stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L,"it has output"=!is.null(result$output),
        "nothing is written before save"=!file.exists(file.path(case,"output.csv")),"the run saves"=saved(result))
      expect_identical(rawfile(file.path(case,"output.csv")),rawfile(expected_csv),paste(label,"saves the authored CSV"))
    } else {
      expected <- findings[findings$case==name,]
      actual <- result$issues
      # A schema finding names the file its run loaded, and the staged truth
      # leaves out the file the capture loaded.
      context <- if(name=="width-noninteger") without_layer(actual$context) else actual$context
      # Each captured padding cause has one independently authored full path.
      expect_identical(
        list(phase=actual$phase,condition=actual$condition,rule=actual$rule,context=context,
          spec_paths=vapply(actual$spec_paths,function(path) paste0('["',path,'"]'),character(1))),
        list(phase=expected$phase,condition=expected$condition,rule=expected$rule,context=expected$context,
          spec_paths=expected$spec_paths),
        paste(label,"has the authored findings"))
      stopifnot_case(label,"a refused run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
        "nothing is written"=!file.exists(file.path(case,"output.csv")))
      if(name=="width-noninteger") {
        expect_identical(check_spec(entry)$issues,actual,paste(label,"is found by the check"))
        stopifnot_case(label,"a static refusal has no observations"=is.null(domain_observations(result)),
          "no input is created"=!file.exists(file.path(case,"input.csv")))
      }
    }
    observed <- domain_observations(result)
    if(!is.null(observed)) {
      # Literal complete arrays avoid locale-dependent regexp offsets for UTF-8
      # report bytes on Windows and retain the independent ledger truth.
      expected <- handler_truth$handlers[handler_truth$case==name]
      stopifnot_case(label,"at most one ledger is authored"=length(expected)<=1L)
      expect_ledger_twice(observed,if(length(expected)) expected else "[]",label)
    }
  }
  expect_original_refusal(directory,"negative-str-pad-width",list(phase="validation",condition="invalid_field_type",
    rule="SCHEMA-12",spec_paths=list("columns.IDVARVAL.derivation.str_pad.width"),
    context='{"actual":"str","expected":"int"}'),layered=TRUE)
  cat("public canonical scalar padding matches complete independent CSV, findings and handlers with missing passthrough and original width failure before input without Python passed\n")
})

local({
  directory <- tempfile("public-text-extract-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  handler_truth <- read_tsv("text-extract-handlers.tsv")
  expect_original_build(directory,"adam-adae-text-cleanup","adae.csv",function(expected) {
    for(column in c("AESEQ","AEREFNUM")) expected[[column]] <- as.double(expected[[column]])
    expected
  },handler_truth)
  expect_original_refusal(directory,"negative-str-uncaptured-group",list(phase="validation",
    condition="regex_group_out_of_range",rule="TEXT-10",spec_paths=list("columns.SITEID.derivation.str_extract.group"),
    context='{"group":2,"group_count":1,"pattern":"^CATH-([^-]+)-[0-9]{4}$"}'))
  failures <- read_tsv("text-extract-diagnostics.tsv")
  for(index in seq_len(nrow(failures))) {
    finding <- failures[index,];case_name <- finding$case
    stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);rejected <- run_spec(file.path(case,"spec.yaml"))
    check_handlers(rejected,handler_truth,case_name)
    expect_case_failure(rejected,finding,paste("the str_extract failure",case_name))
    stopifnot_case(case_name,"nothing is written"=!file.exists(file.path(case,"output.csv")))
  }
  for(case_name in c("optional-unentered","entered-empty","whole-empty","leftmost-alternative","last-repeated-capture","literal-null","nested-case","row-extract")) {
    stopifnot_case(case_name,"the case is copied"=file.copy(file.path(root,"text-extract-cases",case_name),directory,recursive=TRUE))
    case <- file.path(directory,case_name);result <- run_spec(file.path(case,"spec.yaml"))
    check_handlers(result,handler_truth,case_name)
    stopifnot_case(case_name,"the run has no issues"=nrow(result$issues)==0L,
      "nothing is written before save"=!file.exists(file.path(case,"output.csv")),"the run saves"=saved(result))
    expect_identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv")),
      paste(case_name,"saves the authored CSV"))
    if(case_name=="optional-unentered") expect_identical(result$output$RESULT,NA_character_,"an unentered group is missing")
    if(case_name %in% c("entered-empty","whole-empty")) expect_identical(result$output$RESULT,"",
      paste(case_name,"captures empty text"))
  }
  cat("public str_extract matches unchanged ADAE bytes and complete independent failures/captures without Python passed\n")
})

local({
  directory <- tempfile("yamaa-template-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  handlers <- read_tsv("text-template-handlers.tsv")
  paths <- read_tsv("text-template-directories.tsv")
  directories <- setNames(paths$directory,paths$case)
  expect_original_build(directory,"adam-adsl-site-parse","adsl.csv",handlers=handlers)
  name <- "negative-subject-reference"
  stopifnot("the case is copied"=file.copy(file.path(root,"cases",name),directory,recursive=TRUE))
  case <- file.path(directory,name);unlink(file.path(case,"input"),recursive=TRUE)
  checked <- check_spec(file.path(case,"spec.yaml"))$issues;result <- run_spec(file.path(case,"spec.yaml"))
  expect_identical(as.list(checked[c("phase","condition","rule","spec_paths","context")]),
    list(phase="validation",condition="invalid_string_template",rule="TEXT-3",
      spec_paths=list("columns.SUBJREF.derivation.str_template"),
      context='{"placeholder":"SITEID + \':\' + SUBJID","reason":"invalid_placeholder"}'),
    paste(name,"is its one static finding"))
  expect_identical(result$issues,checked,paste(name,"is refused by the run as by the check"))
  stopifnot_case(name,"a refused run has no output"=is.null(result$output),
    "a static refusal has no observations"=is.null(domain_observations(result)),"its save is refused"=save_refused(result))
  failures <- read_tsv("text-template-diagnostics.tsv")
  failures$rule[!nzchar(failures$rule)] <- NA_character_
  # Copy a template case into its own directory, named for the case.
  copy_case <- function(name) {
    stopifnot_case(name,"the case is copied"=file.copy(file.path(root,"text-template-cases",directories[[name]]),directory,recursive=TRUE))
    case <- file.path(directory,name)
    if(directories[[name]]!=name) stopifnot_case(name,"the case takes its name"=
      file.rename(file.path(directory,directories[[name]]),case))
    case
  }
  for(name in unique(failures$case)) {
    case <- copy_case(name)
    result <- run_spec(file.path(case,"spec.yaml"));actual <- result$issues
    expected <- failures[failures$case==name,];check_handlers(result,handlers,name)
    expect_identical(as.list(actual[c("phase","condition","rule","spec_paths","context")]),
      list(phase=expected$phase,condition=expected$condition,rule=expected$rule,
        spec_paths=lapply(expected$spec_path,identity),context=expected$context),
      paste("the str_template failure",name,"has its ordered findings"))
    stopifnot_case(name,"a refused run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
      "nothing is written"=!file.exists(file.path(case,"output.csv")))
  }
  for(name in sort(paths$case)) {
    if(name %in% failures$case || name=="nul-value") next
    # NUL scalar materialization is qualified at pure/native-codec and Python
    # boundaries; the existing R character facade does not admit embedded NUL.
    case <- copy_case(name)
    result <- run_spec(file.path(case,"spec.yaml"));check_handlers(result,handlers,name)
    stopifnot_case(name,"the run has no issues"=nrow(result$issues)==0L,
      "nothing is written before save"=!file.exists(file.path(case,"output.csv")),"the run saves"=saved(result))
    expect_identical(rawfile(file.path(case,"output.csv")),rawfile(file.path(case,"expected.csv")),
      paste(name,"saves the authored CSV"))
    if(name=="empty-template") expect_identical(result$output$RESULT,"","an empty template gives empty text")
    if(name=="empty-string-value") expect_identical(result$output$RESULT,"<>","an empty value fills its placeholder")
    if(name=="missing-null") expect_identical(result$output$RESULT,NA_character_,"a missing value gives missing")
  }
  cat("public str_template matches unchanged site bytes and complete ordered independent findings/handlers without Python passed\n")
})

restore_path()
