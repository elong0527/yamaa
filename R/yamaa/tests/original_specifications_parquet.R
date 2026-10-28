# Installed original specifications, Parquet family: the ordered-sum case
# through Parquet input and output, the closed types and their complete
# refusal reports, the empty-string policy and dictionary (factor) columns.
# Python reads the same truth.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_parquet.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# A complete independently authored original report, decoded and relabelled
# as this R run's, naming `entry` as each record's specification.
public_expected <- function(expected, entry) {
  if (is.character(expected)) expected <- jsonlite::fromJSON(expected, simplifyVector=FALSE)
  expected$runtime <- "r"
  expected$runtime_version <- as.character(getRversion())
  expected$engine_version <- yamaa:::.core_version()
  expected$example <- "domain"
  for (group in c("nodes", "tables", "verifications", "callbacks")) {
    for (index in seq_along(expected[[group]])) expected[[group]][[index]]$specification <- entry
  }
  expected
}
public_observed <- function(result) {
  jsonlite::fromJSON(domain_observations(result),simplifyVector=FALSE)
}
# A decoded report before its save: no artifacts yet.
unsaved_report <- function(expected) {
  expected$artifacts <- list()
  expected
}

local({
  directory <- tempfile("public-parquet-original-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  original <- file.path(root,"cases","adam-adlb-ordered-sum")
  for (input_parquet in c(FALSE,TRUE)) for (output_parquet in c(FALSE,TRUE)) {
    label <- sprintf("ordered sum from %s to %s",if (input_parquet) "Parquet" else "CSV",
      if (output_parquet) "Parquet" else "CSV")
    work <- file.path(directory,paste(input_parquet,output_parquet,sep="-")); dir.create(work)
    dir.create(file.path(work,"input"))
    source <- rawToChar(rawfile(file.path(original,"spec.yaml")))
    if (input_parquet) {
      source <- sub(host_truth[["floating_input_override"]],"input/lb.parquet",source,fixed=TRUE)
      stopifnot_case(label,"the Parquet input is copied"=
        file.copy(file.path(root,"pq","ordered-sum.parquet"),file.path(work,"input","lb.parquet")))
    } else stopifnot_case(label,"the CSV input is copied"=
      file.copy(file.path(original,"input","lb.csv"),file.path(work,"input","lb.csv")))
    if (output_parquet) source <- sub("path: adlb.csv","path: adlb.parquet",source,fixed=TRUE)
    entry <- file.path(work,"spec.yaml"); writeBin(charToRaw(source),entry)
    entry <- normalizePath(entry,winslash="/",mustWork=TRUE)
    expected <- public_expected(rawToChar(rawfile(file.path(root,"expected","adam-adlb-ordered-sum.json"))),entry)
    if (input_parquet) for (index in seq_along(expected$source_reads)) expected$source_reads[[index]]$path <- "input/lb.parquet"
    result <- run_spec(entry)
    stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L,"the run has output"=!is.null(result$output))
    expect_context(public_observed(result),unsaved_report(expected),paste(label,"has the authored unsaved report"))
    unlink(entry); unlink(file.path(work,"input"),recursive=TRUE)
    for (attempt in seq_len(2L)) {
      where <- sprintf("%s, save %d",label,attempt)
      stopifnot_case(where,"the run saves"=saved(result))
      observed <- public_observed(result)
      expect_context(unsaved_report(observed),unsaved_report(expected),paste(where,"keeps the authored report"))
      if (output_parquet) {
        bytes <- rawfile(file.path(work,"adlb.parquet"))
        artifact <- observed$artifacts[[1L]]
        stopifnot_case(where,"the file starts with PAR1"=identical(rawToChar(bytes[1:4]),"PAR1"),
          "the file ends with PAR1"=identical(rawToChar(tail(bytes,4L)),"PAR1"),
          "the artifact is Parquet"=identical(artifact$profile,"parquet"),
          "the artifact counts the file's bytes"=artifact$byte_length==length(bytes),
          "the artifact records each row and its footer"=length(artifact$records)==artifact$row_count+1L)
      } else {
        expect_identical(rawfile(file.path(work,"adlb.csv")),rawfile(file.path(original,"expected","adlb.csv")),
          paste(where,"writes the authored CSV"))
        expect_context(observed,expected,paste(where,"records the authored report"))
      }
    }
  }
  cat("public R Parquet file input/output preserve complete original reports and held saves without Python\n")
})

local({
  directory <- tempfile("public-parquet-refusals-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  entry <- file.path(directory,"spec.yaml")
  source <- paste0('yamaa: "1.0"\ntype: spec\ndomain: TEST\nbase: SRC\n',
    'input: {SRC: input.parquet}\nkeys: [ID]\n',
    'columns:\n  - {name: ID, type: int, derivation: "1"}\n',
    'output: {path: output.csv, columns: [ID]}\n')
  writeBin(charToRaw(source),entry)
  entry <- normalizePath(entry,winslash="/",mustWork=TRUE)
  cases <- list(
    c("utf8","source_parquet_invalid","PARQUET-5",'"dataset":"SRC","path":"input.parquet"'),
    c("utf8-bool","source_field_type_unsupported","PARQUET-7",'"dataset":"SRC","field":"OTHER","path":"input.parquet","stored_type":"bool"'),
    c("utf8-time","source_field_value_invalid","PARQUET-9",'"dataset":"SRC","field":"OTHER","path":"input.parquet","record":1,"value":1'),
    c("empty-name","source_field_name_empty","PARQUET-6",'"dataset":"SRC","field":1,"path":"input.parquet"'),
    c("duplicate","source_field_name_duplicate","PARQUET-6",'"dataset":"SRC","field":"I","path":"input.parquet"'),
    c("mixed-struct","source_field_type_unsupported","PARQUET-7",'"dataset":"SRC","field":"S","path":"input.parquet","stored_type":"struct<left: int64, right: string>"'),
    c("mixed-map","source_field_type_unsupported","PARQUET-7",'"dataset":"SRC","field":"M","path":"input.parquet","stored_type":"map<string, int64 (\'M\')>"'),
    c("mixed-empty","source_field_name_empty","PARQUET-6",'"dataset":"SRC","field":1,"path":"input.parquet"'),
    c("mixed-duplicate","source_field_name_duplicate","PARQUET-6",'"dataset":"SRC","field":"I","path":"input.parquet"')
  )
  unsupported_types <- c(uint64="uint64",binary="binary",
    timezone="timestamp[us, tz=UTC]",list="list<element: int64>",
    "fixed-list"="fixed_size_list<element: int64>[2]",struct="struct<item: int64>",decimal="decimal128(10, 2)",
    # Types that the pinned Arrow and Parquet releases do not know (#3155).
    decimal32="decimal32(9, 2)",decimal64="decimal64(18, 2)","list-view"="list_view<element: int64>",
    "large-list-view"="large_list_view<element: int64>",geometry="binary",geography="binary",
    variant="struct<metadata: binary not null, value: binary not null>")
  for(name in names(unsupported_types)) cases[[length(cases)+1L]] <- c(
    paste0("unsupported-",name),"source_field_type_unsupported","PARQUET-7",
    paste0('"dataset":"SRC","field":"FIELD","path":"input.parquet","stored_type":"',unsupported_types[[name]],'"'))
  for (test in cases) {
    label <- sprintf("the Parquet file %s",test[[1L]])
    stopifnot_case(label,"the input is copied"=
      file.copy(file.path(root,"pq",paste0(test[[1L]],".parquet")),file.path(directory,"input.parquet"),overwrite=TRUE))
    result <- run_spec(entry)
    finding <- paste0('{"condition":"',test[[2L]],'","context":{',test[[4L]],'},"phase":"ingest","rule":"',test[[3L]],'","spec_paths":["input.SRC.path"]}')
    expected <- paste0('{"artifacts":[],"backend":"rust","callbacks":[],"diagnostics":[',finding,'],"engine_version":"',yamaa:::.core_version(),'","error":null,"example":"',test[[1L]],'","handler_counts":[],"nodes":[{"diagnostics":[',finding,'],"handler_counts":[],"outcome":"failure","specification":"spec.yaml","unsupported":[]}],"outcome":"failure","report_version":"0.3.0-draft","runtime":"r","runtime_version":"',as.character(getRversion()),'","source_reads":[{"base_directory":".","condition":null,"outcome":"captured","path":"input.parquet","snapshots_created":1}],"tables":[],"unsupported":[],"verifications":[]}')
    expected <- public_expected(expected,entry)
    stopifnot_case(label,"a refused file leaves no output"=is.null(result$output))
    expect_context(public_observed(result),expected,paste(label,"has the complete refusal report"))
    stopifnot_case(label,"its save is refused"=save_refused(result),
      "nothing is written"=!file.exists(file.path(directory,"output.csv")))
  }
  # 0.4.0 refused this pyarrow int32 field. An unannotated INT32 is a signed
  # 32-bit integer, so it reads as int (PARQUET-7, #2940); R exchanges int as
  # double.
  stopifnot("the int32 input is copied"=
    file.copy(file.path(root,"pq","unsupported-int32.parquet"),file.path(directory,"input.parquet"),overwrite=TRUE))
  int32 <- file.path(directory,"int32.yaml")
  writeBin(charToRaw('{"yamaa":"1.0","type":"spec","domain":"TEST","input":{"SRC":"input.parquet"},"keys":["FIELD"],"columns":[{"name":"FIELD","type":"int","derivation":"SRC.FIELD"}],"output":{"path":"output.csv","columns":["FIELD"]}}'),int32)
  result <- run_spec(int32)
  stopifnot("an int32 field reads without issues"=nrow(result$issues)==0L)
  expect_identical(result$output$FIELD,1,"an int32 field reads as int")
  cat("public R Parquet closed types and exact complete refusal reports passed without Python\n")
})

local({
  directory <- tempfile("public-parquet-empty-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  for (policy in c("missing","present")) {
    label <- paste("the empty-string policy",policy)
    entry <- file.path(directory,"spec.yaml")
    source <- paste0('{"yamaa":"1.0","type":"spec","domain":"TEST","input":{"SRC":{"path":"input.PARQUET","empty_string":"',policy,'"}},"keys":["I"],"columns":[{"name":"I","type":"int","derivation":"SRC.I"},{"name":"S","type":"str","derivation":"SRC.S"}],"output":{"path":"output.csv","columns":["I","S"]}}')
    writeBin(charToRaw(source),entry)
    stopifnot_case(label,"the input is copied"=
      file.copy(file.path(root,"pq","text.parquet"),file.path(directory,"input.PARQUET"),overwrite=TRUE))
    result <- run_spec(entry)
    stopifnot_case(label,"the run has no issues"=nrow(result$issues)==0L)
    expected <- charToRaw(if (policy=="missing") "I,S\n1,\n2,X\n" else 'I,S\n1,""\n2,X\n')
    unlink(entry); unlink(file.path(directory,"input.PARQUET")); invisible(gc())
    for (attempt in seq_len(2L)) {
      stopifnot_case(sprintf("%s, save %d",label,attempt),"the held run saves"=saved(result))
      expect_identical(rawfile(file.path(directory,"output.csv")),expected,
        sprintf("%s, save %d writes the exact bytes",label,attempt))
    }
  }
  cat("public R Parquet empty-string policy and retained exact saved bytes passed without Python\n")
})

local({
  # arrow writes a factor as a BYTE_ARRAY String field whose embedded Arrow
  # schema names a dictionary. A layout is not a type (PARQUET-7), so each
  # field reads as its text (#2940). arrow is a suggested package: where it is
  # not installed, this control is skipped.
  if (!requireNamespace("arrow", quietly = TRUE)) {
    message("Skipped the Parquet factor columns of original_specifications_parquet.R: arrow, a suggested package, is not installed")
    return(invisible())
  }
  directory <- tempfile("public-parquet-factors-"); dir.create(directory)
  on.exit(unlink(directory,recursive=TRUE))
  text <- c("b",NA,"","a","b","\u03b2")
  levels <- c("a","b","","\u03b2")
  path <- file.path(directory,"input.parquet")
  arrow::write_parquet(data.frame(ID=as.character(1:6),FACTOR=factor(text,levels=levels),
    ORDERED=factor(text,levels=levels,ordered=TRUE),stringsAsFactors=FALSE),path)
  stored <- arrow::read_parquet(path,as_data_frame=FALSE)$schema
  stopifnot("arrow stores a factor as a dictionary"=inherits(stored$FACTOR$type,"DictionaryType"),
    "arrow stores an ordered factor as a dictionary"=inherits(stored$ORDERED$type,"DictionaryType"))
  entry <- file.path(directory,"spec.yaml")
  writeBin(charToRaw('{"yamaa":"1.0","type":"spec","domain":"TEST","input":{"SRC":{"path":"input.parquet","empty_string":"present"}},"keys":["ID"],"columns":[{"name":"ID","type":"str","derivation":"SRC.ID"},{"name":"FACTOR","type":"str","derivation":"SRC.FACTOR"},{"name":"ORDERED","type":"str","derivation":"SRC.ORDERED"}],"output":{"path":"output.csv","columns":["ID","FACTOR","ORDERED"]}}'),entry)
  result <- run_spec(entry)
  stopifnot("dictionary columns read without issues"=nrow(result$issues)==0L)
  expect_identical(as.list(result$output),list(ID=as.character(1:6),FACTOR=text,ORDERED=text),
    "dictionary columns read as their text")
  cat("public R Parquet factor columns read as text passed without Python\n")
})

restore_path()
