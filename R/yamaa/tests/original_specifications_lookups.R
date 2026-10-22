# Installed original specifications, lookup family: completed-template SELF
# donors, intermediate admission, donor grain, correlated donor filters, case
# intermediates and case predicates, against exact CSV, complete findings and
# handler accounting. Python reads the same truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_lookups.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# A run that saves `expected`, the exact bytes of its result.csv, and reports
# the authored handler `ledger`.
expect_lookup_run <- function(result, directory, expected, ledger, label) {
  stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L,"the run saves"=saved(result))
  expect_identical(rawfile(file.path(directory,"result.csv")),expected,paste(label,"saves the exact CSV"))
  if(!is.null(ledger)) check_handler_ledger(domain_observations(result),ledger,label)
}

local({
  body <- host_truth[["section_order_yaml"]]
  directory <- tempfile("yamaa-self-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n"),file.path(directory,"src.csv"))
  writeBin(charToRaw("ID,K\nc,x\nd,x\n"),file.path(directory,"other.csv"))
  expect_lookup_run(run_spec(entry),directory,fixture_bytes(host_truth[["section_order_csv"]]),
    "[{\"spec_path\":\"rows.grouped.derivations.V.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows.grouped.derivations.V.absent\",\"handler\":\"absent\",\"count\":0},{\"spec_path\":\"rows.added.derivations.V.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":2},{\"spec_path\":\"rows.added.derivations.V.absent\",\"handler\":\"absent\",\"count\":0},{\"spec_path\":\"columns.FINAL.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5},{\"spec_path\":\"columns.FINAL.derivation.source.absent\",\"handler\":\"absent\",\"count\":0},{\"spec_path\":\"columns.FIRST.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":5},{\"spec_path\":\"columns.FIRST.derivation.source.absent\",\"handler\":\"absent\",\"count\":0}]",
    "completed-template SELF borrowing")

  unlink(file.path(directory,fixture_strings(host_truth[["section_case_files"]])))
  writeBin(charToRaw(sub("SECTION: \"'first'\"","SECTION: P.K",body,fixed=TRUE)),entry)
  expected <- list(phase="validation",condition="phase_boundary",rule="LOOKUP-7",spec_paths=list("rows.collected.derivations.SECTION.source"),context="{\"available_phase\":\"completed_prior_row_template\",\"identifier\":\"P.K\",\"required_phase\":\"row_construction\"}")
  check_issue_frame(check_spec(entry)$issues,expected,"the check of a phase boundary")
  rejected <- run_spec(entry);check_issue_frame(rejected$issues,expected,"the run of a phase boundary")
  stopifnot("a refused run has no output"=is.null(rejected$output),
    "a static refusal has no observations"=is.null(domain_observations(rejected)),
    "its save is refused"=save_refused(rejected),"no input is created"=!file.exists(file.path(directory,"src.csv")),
    "nothing is written"=!file.exists(file.path(directory,"result.csv")))
  cat("completed-template SELF borrowing, complete handlers and pre-study phase refusal passed without Python\n")
})

local({
  body <- host_truth[["duplicate_lookup_yaml"]]
  directory <- tempfile("yamaa-intermediate-admission-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID\n2\n1\n3\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["duplicate_donor_csv"]]),file.path(directory,"donor.csv"))
  expect_lookup_run(run_spec(entry),directory,fixture_bytes(host_truth[["duplicate_lookup_csv"]]),
    '[{"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},{"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":1},{"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},{"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":1}]',
    "declared intermediate keys")
  unlink(c(file.path(directory,"src.csv"),file.path(directory,"donor.csv"),file.path(directory,"result.csv")))
  cases <- list(
    list(remove=host_truth[["lookup_first_clause"]],condition="rename_only_intermediate",rule="LOOKUP-4",context='{"dataset":"DONOR","intermediate":"P"}'),
    list(remove="    keep: first\n",condition="unpaired_fields",rule="LOOKUP-10",context='{"declared":["order_by"],"intermediate":"P","missing":["keep"]}'),
    list(remove="    order_by: [DONOR.SEQ]\n",condition="unpaired_fields",rule="LOOKUP-10",context='{"declared":["keep"],"intermediate":"P","missing":["order_by"]}'))
  for(case in cases) {
    label <- paste("an intermediate without",deparse(case$remove))
    writeBin(charToRaw(sub(case$remove,"",body,fixed=TRUE)),entry)
    expected <- list(phase="validation",condition=case$condition,rule=case$rule,
      spec_paths=list("intermediates.P"),context=case$context)
    checked <- check_spec(entry);check_issue_frame(checked$issues,expected,paste("the check of",label))
    rejected <- run_spec(entry);check_issue_frame(rejected$issues,expected,paste("the run of",label))
    stopifnot_case(label,"a refused run has no output"=is.null(rejected$output),
      "a static refusal has no observations"=is.null(domain_observations(rejected)),
      "its save is refused"=save_refused(rejected),"no input is created"=!file.exists(file.path(directory,"src.csv")))
  }
  cat("declared intermediate keys, stable shared selection and complete declaration failures passed without Python\n")
})

local({
  body <- host_truth[["lookup_filter_yaml"]]
  directory <- tempfile("yamaa-primary-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(fixture_bytes(host_truth[["filtered_donor_csv"]]),file.path(directory,"src.csv"))
  stopifnot("the donor grain checks"=nrow(check_spec(entry)$issues)==0L)
  expect_lookup_run(run_spec(entry),directory,fixture_bytes(host_truth[["filtered_lookup_csv"]]),
    '[{"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":2},{"spec_path":"columns.A.derivation.source.absent","handler":"absent","count":1},{"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":2},{"spec_path":"columns.B.derivation.source.absent","handler":"absent","count":1}]',
    "same-input donor grain")
  cat("same-input donor grain, shared missing field and complete handler accounting passed without Python\n")
})

local({
  directory <- tempfile("yamaa-correlated-donors-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw("ID,SEQ,CUT\n1,1,2\n1,2,4\n3,1,2\n1,3,\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["ordered_donor_csv"]]),file.path(directory,"donor.csv"))
  writeBin(charToRaw("ID,SEQ,CUT\n1,1,4\n3,1,2\n"),file.path(directory,"alt.csv"))
  writeBin(fixture_bytes(host_truth[["lookup_cutoff_yaml"]]),entry)
  expect_lookup_run(run_spec(entry),directory,fixture_bytes(host_truth[["lookup_cutoff_csv"]]),
    "[{\"spec_path\":\"columns.A.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.A.derivation.source.absent\",\"handler\":\"absent\",\"count\":2},{\"spec_path\":\"columns.B.derivation.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.B.derivation.source.absent\",\"handler\":\"absent\",\"count\":2}]",
    "a correlated donor cutoff")
  unlink(file.path(directory,"result.csv"))
  writeBin(fixture_bytes(host_truth[["lookup_alternative_yaml"]]),entry)
  expect_lookup_run(run_spec(entry),directory,charToRaw("ID,SEQ,A,B\n1,1,three,\n3,1,,\n"),
    "[{\"spec_path\":\"rows.alternate.derivations.A.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows.alternate.derivations.A.source.absent\",\"handler\":\"absent\",\"count\":1},{\"spec_path\":\"rows.alternate.derivations.B.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"rows.alternate.derivations.B.source.absent\",\"handler\":\"absent\",\"count\":1}]",
    "a correlated donor of the current row driver")
  unlink(file.path(directory,"result.csv"))
  writeBin(charToRaw(sub("D.TIME < ALT.CUT","D.TIME < SRC.CUT",host_truth[["lookup_alternative_yaml"]],fixed=TRUE)),entry)
  rejected <- run_spec(entry)
  check_issue_frame(rejected$issues,list(phase="validation",condition="unknown_field",rule="LOOKUP-7",spec_paths=list("intermediates.P.filter"),context='{"identifier":"SRC.CUT","intermediate":"P"}'),
    "a filter naming another driver's field")
  stopifnot("a refused run has no output"=is.null(rejected$output),"its save is refused"=save_refused(rejected),
    "nothing is written"=!file.exists(file.path(directory,"result.csv")))
  cat("correlated donor filters, actual current driver, complete findings and handler accounting passed without Python\n")
})

local({
  body <- host_truth[["lookup_fallback_yaml"]]
  directory <- tempfile("yamaa-case-intermediates-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE),add=TRUE)
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw(body),entry)
  writeBin(charToRaw("ID,SELECT\n1,Y\n2,N\n3,Y\n"),file.path(directory,"src.csv"))
  writeBin(fixture_bytes(host_truth[["fallback_donor_csv"]]),file.path(directory,"donor.csv"))
  expect_lookup_run(run_spec(entry),directory,charToRaw("ID,A,B\n1,first,\n2,skip,skip\n3,,\n"),
    "[{\"spec_path\":\"columns.A.derivation.case[0].then.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.A.derivation.case[0].then.source.absent\",\"handler\":\"absent\",\"count\":1},{\"spec_path\":\"columns.B.derivation.case[0].then.source.multiple_matches\",\"handler\":\"multiple_matches\",\"count\":1},{\"spec_path\":\"columns.B.derivation.case[0].then.source.absent\",\"handler\":\"absent\",\"count\":1}]",
    "a nested case intermediate source")
  # 0.5 only, until #3447 deletes the 0.5 path: a 0.5 `absent` answers a
  # variable the lookup does not hold, and a donor without a record is the
  # intermediate's own `no_match` (D67).
  legacy <- host_truth[["lookup_fallback_0_5_yaml"]]
  for (variable in c("P.A", "P.ABSENT")) {
    writeBin(charToRaw(sub("then: P.A",paste0("then: {source: {variable: ",variable,", absent: fallback}}"),legacy,fixed=TRUE)),entry)
    admitted <- run_spec(entry)
    expected <- if (variable == "P.A") "ID,A,B\n1,first,\n2,skip,skip\n3,,\n" else host_truth[["fallback_lookup_csv"]]
    count <- if (variable == "P.A") "0" else "2"
    label <- paste("an absent fallback for",variable)
    expect_lookup_run(admitted,directory,charToRaw(expected),NULL,label)
    handler <- paste0('{"count":',count,',"handler":"absent","spec_path":"columns.A.derivation.case[0].then.source.absent"}')
    stopifnot_case(label,"the absent handler is counted"=grepl(handler,domain_observations(admitted),fixed=TRUE))
  }
  # On a 1.x file the intermediate answers the donor without a record, and its
  # answer counts under `absent` at the read, the binding's own row (D202); a
  # variable the lookup does not hold fails (D67, LOOKUP-11).
  writeBin(charToRaw(sub("then: P.A","then: {source: {variable: P.A, absent: fallback}}",body,fixed=TRUE)),entry)
  admitted <- run_spec(entry)
  label <- "an absent fallback for P.A in a 1.x file"
  expect_lookup_run(admitted,directory,charToRaw("ID,A,B\n1,first,\n2,skip,skip\n3,,\n"),NULL,label)
  stopifnot_case(label,"the intermediate's answer counts at the binding's absent row"=grepl(
    '{"count":1,"handler":"absent","spec_path":"columns.A.derivation.case[0].then.source.absent"}',
    domain_observations(admitted),fixed=TRUE))
  writeBin(charToRaw(sub("then: P.A","then: {source: {variable: P.ABSENT, absent: fallback}}",body,fixed=TRUE)),entry)
  refused <- run_spec(entry)
  check_issue_frame(refused$issues,list(phase="validation",condition="unknown_field",rule="LOOKUP-11",
    spec_paths=list("columns.A.derivation.case[0].then.source"),context='{"identifier":"P.ABSENT"}'),
    "an absent fallback for a variable the lookup does not hold in a 1.x file")
  stopifnot("a refused run has no output"=is.null(refused$output))
  cat("nested case intermediate source shares lazy kept selection and complete handler accounting without Python\n")
})

local({
  failures <- read.delim(file.path(root,"case-predicate-diagnostics.tsv"),quote="",colClasses="character",check.names=FALSE)
  for (case in list.dirs(file.path(root,"case-predicate-cases"),full.names=TRUE,recursive=FALSE)) {
    label <- paste("the case predicate",basename(case))
    directory <- tempfile("yamaa-case-predicate-");dir.create(directory)
    file.copy(list.files(case,full.names=TRUE),directory)
    entry <- file.path(directory,"spec.yaml")
    result <- run_spec(entry)
    expected <- file.path(directory,"expected.csv")
    if (file.exists(expected)) {
      expect_lookup_run(result,directory,rawfile(expected),NULL,label)
    } else {
      finding <- failures[failures$case==basename(case),,drop=FALSE]
      expect_identical(nrow(finding),1L,paste(label,"has one authored finding"))
      check_issue_frame(result$issues,list(phase=finding$phase,condition=finding$condition,rule=finding$rule,spec_paths=list(finding$spec_path),context=finding$context),
        paste("the issues of",label))
      stopifnot_case(label,"a refused run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
        "nothing is written"=!file.exists(file.path(directory,"result.csv")))
    }
    unlink(directory,recursive=TRUE)
  }
  for (name in fixture_strings(host_truth[["source_selection_case_names"]])) {
    case <- file.path(root,"cases",name)
    directory <- tempfile("yamaa-original-case-");dir.create(directory)
    file.copy(list.files(case,full.names=TRUE),directory,recursive=TRUE)
    result <- run_spec(file.path(directory,"spec.yaml"))
    stopifnot_case(name,"the run has no issues"=nrow(result$issues)==0L,"the run saves"=saved(result))
    for (expected in list.files(file.path(directory,"expected"),pattern="\\.csv$",full.names=TRUE))
      expect_identical(rawfile(file.path(directory,basename(expected))),rawfile(expected),
        paste(name,"saves the unchanged",basename(expected)))
    unlink(directory,recursive=TRUE)
  }
  cat("case predicates, branch filters and unchanged original CSV truth passed without Python\n")
})

restore_path()
