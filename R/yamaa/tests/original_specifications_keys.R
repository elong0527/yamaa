# Installed original specifications, flag and key family: declarative flags,
# common input record rows, scalar intermediate keys and retained filter
# types, against complete findings, CSV and reached handler ledgers on two
# attempts. Python reads the same truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_keys.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# A run's complete authored handler ledger, or none where the run has no
# observations and the truth authors none.
check_observed_ledger <- function(observed, ledger, label) {
  if(is.null(observed)) expect_identical(ledger,"[]",paste(label,"authors no ledger without observations"))
  else expect_ledger_twice(observed,ledger,label)
}

local({
  source(file.path(root,"flag-truth.R"),local=TRUE)
  expect_identical(length(flag_truth),33L,"the authored flag cases")
  directory <- tempfile("flag-originals-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  for(name in names(flag_truth)) {
    truth <- flag_truth[[name]]
    stopifnot_case(name,"the case is copied"=file.copy(file.path(root,truth$directory,name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    # A witness is staged from its capture; a benchmark runs as it is written,
    # in either language, whose findings and handlers are the same (#3422).
    if(identical(truth$directory,"flags"))
      expect_identical(rawfile(entry),charToRaw(enc2utf8(truth$specification)),paste(name,"is staged from its capture"))
    files <- list.files(case,recursive=TRUE,full.names=TRUE)
    retained <- lapply(files,rawfile)
    output <- file.path(case,truth$output)
    for(attempt in seq_len(2L)) {
      label <- sprintf("the flag case %s, attempt %d",name,attempt)
      stopifnot_case(label,"nothing is written before the run"=!file.exists(output))
      result <- run_spec(entry)
      issues <- result$issues
      if(truth$schema_failure) issues$context <- without_layer(issues$context)
      check_issue_frame(issues,truth$findings,paste("the issues of",label))
      wire <- domain_observations(result)
      check_observed_ledger(wire,truth$handlers,label)
      if(nzchar(truth$expected_csv)) {
        stopifnot_case(label,"the output is a data frame"=is.data.frame(result$output),"the run saves"=saved(result))
        expect_identical(rawfile(output),rawfile(file.path(case,truth$expected_csv)),paste(label,"saves the authored CSV"))
        unlink(output)
      } else stopifnot_case(label,"a failed run has no output"=is.null(result$output),"its save is refused"=save_refused(result))
      stopifnot_case(label,"nothing is left written"=!file.exists(output))
      expect_identical(lapply(files,rawfile),retained,paste(label,"leaves the case's files"))
      if(truth$schema_failure) stopifnot_case(label,"a schema failure has no observations"=is.null(wire))
    }
  }
  cat("declarative flags: six unchanged positives/six negatives and 21 authored complete captures match findings/CSV/handlers on two attempts without Python; schema provenance and two held report/case gates remain explicit\n")
})

local({
  source(file.path(root,"row-intermediate-truth.R"),local=TRUE)
  directory <- tempfile("public-row-intermediates-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(row_intermediate_truth)) {
    truth <- row_intermediate_truth[[name]]
    label <- paste("the row case",name)
    if(identical(name,"different-row-drivers")) stopifnot_case(label,"the case succeeds without issues"=
      truth$success && length(truth$issues$phase)==0L)
    stopifnot_case(label,"the case is copied"=file.copy(file.path(root,"row-cases",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    result <- run_spec(entry);check_issue_frame(result$issues,truth$issues,paste("the issues of",label))
    observed <- domain_observations(result)
    # Compare complete arrays literally; authored paths can contain brackets.
    if(!is.null(observed)) expect_ledger_twice(observed,truth$ledger,label)
    output <- file.path(case,truth$output)
    stopifnot_case(label,"nothing is written before save"=!file.exists(output))
    if(truth$success) {
      stopifnot_case(label,"the run has output"=!is.null(result$output))
      for(attempt in seq_len(2L)) {
        stopifnot_case(sprintf("%s, save %d",label,attempt),"the run saves"=saved(result))
        expect_identical(rawfile(output),rawfile(file.path(case,"expected.csv")),
          sprintf("%s, save %d writes the unchanged bytes",label,attempt))
      }
    } else {
      stopifnot_case(label,"a failed run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
        "nothing is written"=!file.exists(output))
    }
    if(name %in% fixture_strings(host_truth[["row_driver_cases"]])) {
      check_issue_frame(check_spec(entry)$issues,truth$issues,paste("the check of",label))
    }
  }
  original <- file.path(directory,"adam-adlb-supplb-padded-key")
  unlink(file.path(original,"input"),recursive=TRUE)
  stopifnot("the padded-key case checks without its input"=nrow(check_spec(file.path(original,"spec.yaml"))$issues)==0L,
    "the check creates no input"=!file.exists(file.path(original,"input")))
  cat("common input record rows match unchanged padded-key bytes, complete independent findings and reached handler ledgers without Python; EOT intermediate/grouped named-source execution passed\n")
})

local({
  source(file.path(root,"key-truth.R"),local=TRUE)
  expect_identical(length(key_truth),19L,"the authored key cases")
  directory <- tempfile("public-intermediate-keys-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(name in names(key_truth)) {
    truth <- key_truth[[name]]
    stopifnot_case(name,"the case is copied"=file.copy(file.path(root,"keys",name),directory,recursive=TRUE))
    case <- file.path(directory,name);entry <- file.path(case,"spec.yaml")
    written <- rawfile(entry);output <- file.path(case,"out.csv")
    for(attempt in seq_len(2L)) {
      label <- sprintf("the key case %s, attempt %d",name,attempt)
      stopifnot_case(label,"nothing is written before the run"=!file.exists(output))
      result <- run_spec(entry);check_issue_frame(result$issues,truth$issues,paste("the issues of",label))
      check_observed_ledger(domain_observations(result),truth$handlers,label)
      if(truth$success) {
        stopifnot_case(label,"the run has output"=!is.null(result$output),"the run saves"=saved(result))
        expect_identical(rawfile(output),rawfile(file.path(case,"expected.csv")),paste(label,"saves the authored CSV"))
        unlink(output)
      } else {
        stopifnot_case(label,"a failed run has no output"=is.null(result$output),"its save is refused"=save_refused(result))
      }
      stopifnot_case(label,"nothing is left written"=!file.exists(output),
        "the run leaves its specification"=identical(rawfile(entry),written))
    }
  }
  cat("all 19 unchanged scalar-key captures match complete ordered findings, CSV and handler ledgers twice without Python passed\n")
})

local({
  source(file.path(root,"key-truth.R"),local=TRUE)
  original <- file.path(root,"keys","dependency_before_bad_filter")
  type_truth <- list(phase="validation",condition="incompatible_input_type",rule="FORMULA-5",
    spec_paths=list("intermediates[0].filter"),context='{"left_type":"int","right_type":"str"}')
  directory <- tempfile("retained-filter-types-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for(variant in c("duplicate","resolved","empty_donor","empty_driver","missing_filter_operand","unused","unread_branch","boolean_missing")) {
    label <- paste("the filter variant",variant)
    stopifnot_case(label,"the case is copied"=file.copy(original,directory,recursive=TRUE))
    case <- file.path(directory,"dependency_before_bad_filter")
    entry <- file.path(case,"spec.yaml");output <- file.path(case,"out.csv")
    written <- rawToChar(rawfile(entry))
    if(variant!="duplicate") writeBin(charToRaw("ID,N\na,7\n"),file.path(case,"src.csv"))
    if(variant=="empty_donor") writeBin(charToRaw("K,KINT,KNUM,VAL\n"),file.path(case,"tab.csv"))
    if(variant=="empty_driver") writeBin(charToRaw("ID,N\n"),file.path(case,"src.csv"))
    if(variant %in% c("missing_filter_operand","boolean_missing"))
      writeBin(charToRaw('K,KINT,KNUM,VAL\n"       7",,7,A\n'),file.path(case,"tab.csv"))
    if(variant=="unused") written <- gsub("derivation: LOOK.VAL","derivation: {literal: ok}",written,fixed=TRUE)
    if(variant=="unread_branch") written <- gsub("derivation: LOOK.VAL",
      "derivation: {case: [{when: 'FALSE', then: LOOK.VAL}, {otherwise: {literal: ok}}]}",written,fixed=TRUE)
    if(variant=="boolean_missing") written <- sub("TAB.KINT > 'bad'","FALSE AND TAB.KINT > 'bad'",written,fixed=TRUE)
    writeBin(charToRaw(written),entry)
    result <- run_spec(entry)
    if(variant=="unused") {
      # Every specification is pruned after composition (LAYER-11, LAYER-12): no
      # column reads LOOK, so pruning discards the intermediate together with its
      # ill-typed filter and the specification builds with no issues instead of
      # refusing the dead declaration.
      stopifnot_case(label,"the pruned specification builds"=nrow(result$issues)==0L,
        "it has output"=!is.null(result$output),"it saves"=saved(result))
      expect_identical(rawfile(output),charToRaw("ID,V,V2\na,ok,ok\n"),paste(label,"saves the literal"))
      unlink(file.path(case,c("src.csv","tab.csv")))
      stopifnot_case(label,"the pruned specification checks"=nrow(check_spec(entry)$issues)==0L)
      unlink(case,recursive=TRUE)
      next
    }
    check_issue_frame(result$issues,if(variant=="duplicate") key_truth$dependency_before_bad_filter$issues else type_truth,
      paste("the run of",label))
    stopifnot_case(label,"a refused run has no output"=is.null(result$output))
    for(i in seq_len(2L)) stopifnot_case(sprintf("%s, save %d",label,i),"its save is refused"=save_refused(result))
    stopifnot_case(label,"nothing is written"=!file.exists(output))
    unlink(file.path(case,c("src.csv","tab.csv")))
    check_issue_frame(check_spec(entry)$issues,type_truth,paste("the check of",label))
    unlink(case,recursive=TRUE)
  }
  cat("retained filter types, original key ordering and check without source files passed\n")
})

restore_path()
