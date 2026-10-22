# Installed original specifications, source family: raw source filters and
# record positions, independent source selection and completed-row selected
# assertions, against exact records, findings, handler ledgers, reads and
# retained saved bytes. Python reads the same truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_sources.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

local({
  directory <- tempfile("public-source-positions-");dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  entry <- file.path(directory,"spec.yaml");data <- file.path(directory,"input.csv");output <- file.path(directory,"output.csv")
  writeLines(c('yamaa: "1.0"',"type: spec","domain: TEST","keys: [ID]",
    "input: {SRC: {path: input.csv, ordinal: REC}}", "filter: \"SRC.KEEP = 'Y'\"",
    "output: {path: output.csv, columns: [ID, POSITION], order_by: [SORT]}", "columns:",
    "  - {name: ID, type: str, derivation: SRC.ID}",
    "  - {name: POSITION, type: int, derivation: SRC.REC}",
    "  - {name: SORT, type: int, derivation: SRC.SORT}"),entry,useBytes=TRUE)
  inputs <- c(host_truth[["filtered_multiline_csv"]],"ID,SORT,KEEP,NOTE\n")
  expected <- c("ID,POSITION\nc,3\nb,2\n","ID,POSITION\n")
  for(i in seq_along(inputs)) {
    label <- if(i==1L) "the multiline source" else "the header-only source"
    writeBin(charToRaw(inputs[[i]]),data)
    stopifnot_case(label,"the source checks"=nrow(check_spec(entry)$issues)==0L)
    result <- run_spec(entry)
    stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L,"it has output"=!is.null(result$output),
      "nothing is written before save"=!file.exists(output))
    if(i==1L) expect_identical(list(result$output$ID,as.character(result$output$POSITION)),list(c("c","b"),c("3","2")),
      paste(label,"keeps its record positions in presentation order"))
    else expect_identical(nrow(result$output),0L,paste(label,"builds no rows"))
    unlink(data)
    for(attempt in seq_len(2L)) {
      stopifnot_case(sprintf("%s, save %d",label,attempt),"the held run saves"=saved(result))
      expect_identical(rawfile(output),charToRaw(expected[[i]]),sprintf("%s, save %d writes the exact CSV",label,attempt))
    }
    unlink(output)
  }
  writeBin(charToRaw("ID,REC,SORT,KEEP\n"),data)
  result <- run_spec(entry);actual <- result$issues
  expect_identical(as.list(actual[c("phase","condition","rule","spec_paths","context")]),
    list(phase="validation",condition="ordinal_field_collision",rule="INPUT-4",spec_paths=list("input.SRC.ordinal"),
      context='{"dataset":"SRC","field":"REC"}'),
    "an ordinal that names a source field")
  stopifnot("a refused run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
    "nothing is written"=!file.exists(output))
  cat("public raw source filter retains multiline record positions and internal presentation order with independent exact cached CSV twice without Python passed\n")
})

# Run one source-selection or completed-assertion case of `label`'s truth:
# its check and run findings, handler ledger, reads and, for a case with
# output, its typed cells and exact saved bytes after `changed` changes.
run_selection_case <- function(label, truth, attempt, changed, normalize = FALSE) {
  where <- sprintf("the %s case %s, attempt %d",label,truth$directory,attempt)
  directory <- tempfile("yamaa-source-selection-");dir.create(directory)
  if(normalize) directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
  file.copy(list.files(file.path(root,paste0(label,"-cases"),truth$directory),full.names=TRUE),directory)
  entry <- file.path(directory,"spec.yaml")
  check_issue_frame(check_spec(entry)$issues,truth$check_issues,paste("the check of",where))
  result <- run_spec(entry)
  check_issue_frame(result$issues,truth$issues,paste("the run of",where))
  report <- domain_observations(result)
  check_handler_ledger(report,truth$handlers,where)
  if (!is.null(truth$verifications)) {
    expected <- gsub('"__SPECIFICATION__"',json_text(entry),truth$verifications,fixed=TRUE)
    stopifnot_case(where,"the report has the authored verifications"=
      grepl(paste0('"verifications":',expected),report,fixed=TRUE))
  }
  stopifnot_case(where,"the report has the authored reads"=grepl(paste0('"source_reads":',truth$source_reads),report,fixed=TRUE),
    "nothing is saved before save"=grepl('"artifacts":[]',report,fixed=TRUE))
  if (!is.null(truth$columns)) {
    check_text_columns(result,truth,where)
    stopifnot_case(where,"the run saves"=saved(result))
    expect_identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv")),
      paste(where,"saves the authored CSV"))
    writeBin(charToRaw("changed after build"),file.path(directory,changed))
    stopifnot_case(where,"the run saves after its source changed"=saved(result))
    expect_identical(rawfile(file.path(directory,"result.csv")),rawfile(file.path(directory,"expected.csv")),
      paste(where,"saves the bytes it read"))
  } else stopifnot_case(where,"a failed run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
    "nothing is written"=!file.exists(file.path(directory,"result.csv")))
  unlink(directory,recursive=TRUE)
}

local({
  # Historical private truth is retained separately from the normative correction.
  source(file.path(root,"case-predicate-dispositions.R"),local=TRUE)
  for (name in names(case_predicate_dispositions)) {
    disposition <- case_predicate_dispositions[[name]]
    expect_identical(disposition[c("classification","rule")],
      list(classification="historical_private_reference_defect",rule="LOOKUP-8"),paste("the disposition of",name))
    expect_identical(rawfile(file.path(root,"case-predicate-history",name,"expected.csv")),
      charToRaw(disposition$historical_csv),paste("the historical CSV of",name))
  }
  for (label in fixture_strings(host_truth[["source_selection_modes"]])) {
    source(file.path(root,paste0(label,"-truth.R")),local=TRUE)
    for (name in names(source_selection_truth)) for (attempt in seq_len(2L)) {
      run_selection_case(label,source_selection_truth[[name]],attempt,"input.csv")
    }
  }
  cat("independent source selection, key-phase ordering, full int64 cells and retained bytes passed without Python\n")
})

local({
  label <- "completed-lookup-assertions"
  source(file.path(root,paste0(label,"-truth.R")),local=TRUE)
  expect_identical(length(completed_lookup_assertion_truth),21L,"the authored completed-row assertion cases")
  for (name in names(completed_lookup_assertion_truth)) for (attempt in seq_len(2L)) {
    run_selection_case(label,completed_lookup_assertion_truth[[name]],attempt,"donor.csv",normalize=TRUE)
  }
  cat("independent completed-row selected assertions retain exact records, effects and Save bytes without Python\n")
})

restore_path()
