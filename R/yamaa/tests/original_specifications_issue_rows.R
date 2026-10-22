# Installed original specifications, paired issue-row family: namespace
# admission, predicate suggestions, intermediate dependencies and row
# drivers, unknown references, row-count lookups, baseline window types, row
# join keys and key expressions. Each case's check and run equal its authored
# issue rows on two attempts, and its output or refusal equals the truth;
# Python reads the same cases.
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped before it changes anything.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped original_specifications_issue_rows.R: its truth is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures",winslash="/",mustWork=TRUE)
for (helper in c("reader.R","context.R","assert.R")) source(file.path(host_fixture_root,helper),local=TRUE)
# Build tools may include Python, but the installed runtime probe must not see it.
restore_path <- without_python()
library(yamaa)
source(file.path(host_fixture_root,"original.R"),local=TRUE)

# A case directory holding its specification, and its inputs where `inputs`.
# Row-count lookups name the specification by its normalized path.
new_case <- function(family, truth, inputs = TRUE) {
  directory <- tempfile(paste0(gsub("_","-",family),"-")); dir.create(directory)
  if (family == "row_count_lookups") directory <- normalizePath(directory,winslash="/",mustWork=TRUE)
  if (inputs) write_files(directory,truth$inputs)
  writeBin(charToRaw(truth$specification),file.path(directory,"spec.yaml"))
  directory
}
# A case without output: no output, a refused save and nothing written.
expect_refused_run <- function(result, path, label) {
  stopifnot_case(label,"a refused run has no output"=is.null(result$output),"its save is refused"=save_refused(result),
    "nothing is written"=!file.exists(path))
}
# A case with output: its save writes the authored CSV.
expect_saved_csv <- function(result, path, csv, label) {
  stopifnot_case(label,"the run saves"=saved(result))
  expect_identical(rawfile(path),charToRaw(csv),paste(label,"saves the authored CSV"))
}

local({
  for (index in seq_along(host_truth[["namespace_admission"]])) {
    truth <- host_truth[["namespace_admission"]][[index]]
    directory <- new_case("namespace_admission",truth,inputs=FALSE)
    entry <- file.path(directory,"spec.yaml")
    original <- rawfile(entry)
    # `{entry}` stands for the specification's absolute path as the engine
    # resolves it, with forward slashes.
    resolved <- normalizePath(entry,winslash="/",mustWork=TRUE)
    expected <- lapply(truth$issues,function(issue) {
      issue$context <- lapply(issue$context,function(value) if (identical(value,"{entry}")) resolved else value)
      issue
    })
    # No input file exists: declaration admission must precede source access.
    for (attempt in 1:2) {
      label <- sprintf("namespace admission case %d, attempt %d",index,attempt)
      expect_issue_rows(check_spec(entry)$issues,expected,paste("the check of",label))
      result <- run_spec(entry)
      expect_issue_rows(result$issues,expected,paste("the run of",label))
      expect_refused_run(result,file.path(directory,"out.csv"),label)
      expect_identical(rawfile(entry),original,paste(label,"leaves its specification"))
    }
    unlink(directory,recursive=TRUE)
  }
  cat("original namespace declarations preserve complete paired public refusals before source reads\n")
})

# Predicate suggestions, row drivers, row-count lookups and baseline window
# types differ only in their truth: each case's inputs exist, and its check
# and run issues, saved CSV or refusal, and for row-count lookups its
# verifications and handler counts, equal the truth on two attempts.
paired <- c(
  predicate_suggestions="original predicate qualified suggestions preserve unknown names and paired output",
  intermediate_row_drivers="source-only named row drivers preserve record order and governed lookup keys",
  row_count_lookups="original row-count lookup predicates preserve typed scopes and paired output",
  baseline_window_types="baseline temporal operands preserve typed refusals and valid carriers")
for (family in names(paired)) local({
  for (index in seq_along(host_truth[[family]])) {
    truth <- host_truth[[family]][[index]]
    directory <- new_case(family,truth)
    entry <- file.path(directory,"spec.yaml")
    for (attempt in 1:2) {
      label <- sprintf("%s case %d, attempt %d",family,index,attempt)
      expect_issue_rows(check_spec(entry)$issues,truth$check_issues,paste("the check of",label))
      result <- run_spec(entry)
      expect_issue_rows(result$issues,truth$issues,paste("the run of",label))
      if (family == "row_count_lookups") {
        report <- jsonlite::fromJSON(domain_observations(result),simplifyVector=FALSE)
        expected <- lapply(truth$verifications,function(record) {
          record$specification <- entry
          record
        })
        expect_identical(report$verifications,expected,paste(label,"reports the authored verifications"))
        expect_identical(report$handler_counts,truth$handler_counts,paste(label,"reports the authored handler counts"))
      }
      output <- file.path(directory,"out.csv")
      if (!is.null(truth$csv)) expect_saved_csv(result,output,truth$csv,label)
      else expect_refused_run(result,output,label)
    }
    unlink(directory,recursive=TRUE)
  }
  cat(paired[[family]],"\n",sep="")
})

local({
  for (index in seq_along(host_truth[["intermediate_dependencies"]])) {
    truth <- host_truth[["intermediate_dependencies"]][[index]]
    directory <- new_case("intermediate_dependencies",truth,inputs=FALSE)
    entry <- file.path(directory,"spec.yaml")
    output <- file.path(directory,"out.csv")
    for (inputs_present in c(FALSE,TRUE)) {
      if (inputs_present) write_files(directory,truth$inputs)
      else if (!length(truth$issues)) next
      for (attempt in 1:2) {
        label <- sprintf("intermediate dependencies case %d %s inputs, attempt %d",index,
          if (inputs_present) "with" else "without",attempt)
        expect_issue_rows(check_spec(entry)$issues,truth$issues,paste("the check of",label))
        result <- run_spec(entry)
        expect_issue_rows(result$issues,truth$issues,paste("the run of",label))
        stopifnot_case(label,"nothing is written before save"=!file.exists(output))
        if (length(truth$issues)) {
          stopifnot_case(label,"a refused run has no output"=is.null(result$output),
            "a static refusal has no observations"=is.null(domain_observations(result)),
            "its save is refused"=save_refused(result))
        } else {
          if (!is.null(truth$output_rows)) expect_identical(list(result$output$K,result$output$XVAL),list("r1",1),
            paste(label,"builds the authored row"))
          expect_saved_csv(result,output,truth$csv,label)
          unlink(output)
        }
      }
    }
    expect_identical(rawfile(entry),charToRaw(truth$specification),sprintf("intermediate dependencies case %d leaves its specification",index))
    unlink(directory,recursive=TRUE)
  }
  cat("original intermediate cycles and forward donor dependencies preserve complete paired findings and saved bytes\n")
})

# Unknown references and row join keys: each case is checked before and
# after its inputs exist, and its runs keep the twelve issue columns.
references <- c(
  diagnostic_sites="original unknown references retain authored identifiers, dataset context and complete findings without Python",
  row_join_keys="implicit row joins preserve raw driver keys while column and named lookups preserve constructed keys without Python")
expect_identical(length(host_truth[["row_join_keys"]]),11L,"the authored row join cases")
for (family in names(references)) local({
  for (index in seq_along(host_truth[[family]])) {
    truth <- host_truth[[family]][[index]]
    directory <- new_case(family,truth,inputs=FALSE)
    entry <- file.path(directory,"spec.yaml")
    case <- sprintf("%s case %d",family,index)
    expect_issue_rows(check_spec(entry)$issues,truth$check_issues,paste("the check without inputs of",case),issue_columns)
    write_files(directory,truth$inputs)
    for (attempt in 1:2) {
      label <- sprintf("%s, attempt %d",case,attempt)
      expect_issue_rows(check_spec(entry)$issues,truth$check_issues,paste("the check of",label),issue_columns)
      result <- run_spec(entry)
      expect_issue_rows(result$issues,truth$issues,paste("the run of",label),issue_columns)
      output <- file.path(directory,if (is.null(truth$output)) "out.csv" else truth$output)
      if (!is.null(truth$csv)) {
        stopifnot_case(label,"the run has output"=!is.null(result$output))
        expect_saved_csv(result,output,truth$csv,label)
        unlink(output)
      } else expect_refused_run(result,output,label)
    }
    unlink(directory,recursive=TRUE)
  }
  cat(references[[family]],"\n",sep="")
})

local({
  for (index in seq_along(host_truth[["key_expressions"]])) {
    truth <- host_truth[["key_expressions"]][[index]]
    directory <- new_case("key_expressions",truth,inputs=FALSE)
    entry <- file.path(directory,"spec.yaml")
    output <- file.path(directory,"out.csv")
    case <- sprintf("key expressions case %d",index)
    # A refusal binding decides from the declarations is reported by the
    # build only once it has captured the inputs.
    if (length(truth$check_issues) && !isTRUE(truth$check_after_capture)) {
      expect_issue_rows(check_spec(entry)$issues,truth$check_issues,paste("the check without inputs of",case))
      expect_issue_rows(run_spec(entry)$issues,truth$check_issues,paste("the run without inputs of",case))
    }
    write_files(directory,truth$inputs)
    for (attempt in 1:2) {
      label <- sprintf("%s, attempt %d",case,attempt)
      expect_issue_rows(check_spec(entry)$issues,truth$check_issues,paste("the check of",label))
      result <- run_spec(entry)
      expect_issue_rows(result$issues,truth$issues,paste("the run of",label))
      stopifnot_case(label,"nothing is written before save"=!file.exists(output))
      if (length(truth$issues)) expect_refused_run(result,output,label)
      else {
        counts <- jsonlite::fromJSON(domain_observations(result),simplifyVector=FALSE)$handler_counts
        expect_identical(counts,truth$handlers,paste(label,"reports the authored handler counts"))
        expect_saved_csv(result,output,truth$csv,label)
        writeBin(charToRaw("changed after build"),file.path(directory,"source.csv"))
        expect_saved_csv(result,output,truth$csv,paste(label,"after its source changed"))
        unlink(output)
        writeBin(charToRaw(truth$inputs[["source.csv"]]),file.path(directory,"source.csv"))
      }
    }
    expect_identical(rawfile(entry),charToRaw(truth$specification),paste(case,"leaves its specification"))
    unlink(directory,recursive=TRUE)
  }
  cat("scalar and window key expressions preserve current scope, raw comparisons, handler counts and saved truth without Python\n")
})

restore_path()
