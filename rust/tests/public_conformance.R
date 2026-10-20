# Installed public conformance: base R and yamaa only, outside R CMD check.
# Expected files never enter the copied execution inputs.
hex <- function(value) paste0(sprintf("%02x", as.integer(value)), collapse = "")
json_text <- function(value) {
  codes <- utf8ToInt(enc2utf8(value))
  escaped <- vapply(codes, function(code) switch(as.character(code),
    "8"="\\b", "9"="\\t", "10"="\\n", "12"="\\f", "13"="\\r", "34"="\\\"", "92"="\\\\",
    if(code < 32L) sprintf("\\u%04x", code) else intToUtf8(code)), character(1L))
  paste0('"', paste0(escaped, collapse = ""), '"')
}
raw_json <- function(value) structure(value, class = "raw_json")
json <- function(value) {
  if (inherits(value, "raw_json")) return(unclass(value))
  if (is.null(value)) return("null")
  if (is.list(value)) {
    values <- vapply(value, json, character(1L))
    if (!is.null(names(value))) return(paste0("{", paste0(vapply(names(value), json_text, character(1L)), ":", values, collapse=","), "}"))
    return(paste0("[", paste0(values, collapse=","), "]"))
  }
  if (length(value) != 1L) return(paste0("[", paste0(vapply(as.list(value), json, character(1L)), collapse=","), "]"))
  if (is.na(value)) return("null")
  if (is.logical(value)) return(if(value) "true" else "false")
  if (is.character(value)) return(json_text(value))
  stop("JSON serialization requires lossless scalar text")
}
write_json <- function(path, value) {
  dir.create(dirname(path), recursive=TRUE, showWarnings=FALSE)
  writeBin(charToRaw(paste0(json(value), "\n")), path)
}
copy_tree <- function(source, target) {
  dir.create(target, recursive=TRUE, showWarnings=FALSE)
  if (!dir.exists(target)) stop("could not create staged R input directory: ", target)
  for (entry in list.files(source, all.files=TRUE, no..=TRUE)) {
    from <- file.path(source, entry)
    to <- file.path(target, entry)
    # The staged trees preserve symbolic links (including broken ones), so
    # recreate links as links instead of following them to their targets.
    link <- Sys.readlink(from)
    if (nzchar(link)) {
      if (!file.symlink(link, to)) stop("could not copy staged R input link: ", from)
    }
    else if (dir.exists(from)) copy_tree(from, to)
    else if (!file.copy(from, to)) stop("could not copy staged R input: ", from)
  }
  invisible(TRUE)
}
move_inputs <- function(source, target) {
  if (!dir.exists(source)) stop("missing fresh R inputs; regenerate the conformance plan: ", source)
  dir.create(dirname(target), recursive=TRUE, showWarnings=FALSE)
  if (file.rename(source, target)) return(invisible(TRUE))
  # file.rename() cannot move a directory across filesystems, so fall back
  # to copying the staged tree into place and deleting the original.
  copy_tree(source, target)
  unlink(source, recursive=TRUE, force=TRUE)
  if (dir.exists(source)) stop("could not move staged R inputs to execution work: ", source)
  invisible(TRUE)
}
# R exchanges int and float as double, so a public table cannot tell them
# apart by class. Each column's kind is its declared type: the
# specification's for an output, CHECK-24's for the run report. A column's
# R class must agree with its kind, and every cell must convert exactly.
report_types <- c(ARTIFACT="str", KIND="str", SPEC_PATH="str", NAME="str", ID="str", TARGET="str",
  RULE="str", SEVERITY="str", OUTCOME="str", CONDITION="str", COUNT="int", TOTAL="int", KEYS="str",
  DETAILS="str")
whole_text <- function(value, kind) {
  # An int is a whole double within 2^53, never negative zero; a temporal
  # value is a whole count of days or seconds.
  limit <- if (identical(kind, "int")) 2^53 else Inf
  if (!is.finite(value) || value != trunc(value) || abs(value) > limit || (value == 0 && 1 / value < 0))
    stop("public ", kind, " cell is not an exact whole value: ", format(value, digits=17L))
  sprintf("%.0f", value)
}
public_table <- function(frame, types) {
  if (is.null(frame)) return(NULL)
  if (!is.data.frame(frame) || !identical(names(types), names(frame)))
    stop("public table columns differ from their declared types")
  for (index in seq_along(frame)) {
    column <- frame[[index]]
    admitted <- switch(types[[index]],
      str=is.character(column) && is.null(attributes(column)),
      int=, float=is.double(column) && is.null(attributes(column)),
      date=identical(class(column), "Date"),
      datetime=identical(class(column), c("POSIXct", "POSIXt")) && identical(attr(column, "tzone"), "UTC"),
      stop("unknown declared column type: ", types[[index]]))
    if (!admitted) stop("public column ", names(frame)[[index]], " is not a ", types[[index]], " column: ",
      paste(class(column), collapse=","))
  }
  cell <- function(column, index, kind) {
    value <- unclass(column)[[index]]
    if (is.na(value)) return(NULL)
    switch(kind, str=hex(charToRaw(enc2utf8(value))), int=whole_text(value, kind),
      float=hex(writeBin(value, raw(), size=8L, endian="big")),
      date=whole_text(as.double(value), kind), datetime=whole_text(value, kind))
  }
  list(columns=lapply(seq_along(frame), function(index) as.list(c(names(frame)[[index]], types[[index]]))),
    rows=lapply(seq_len(nrow(frame)), function(row) lapply(seq_along(frame), function(column) cell(frame[[column]], row, types[[column]]))))
}
# The declared types of the output columns, from the composed specification
# the engine resolves for the entry: its own declarations and its parents'.
declared_types <- function(spec, columns) {
  resolved <- spec$resolve()
  entry <- Filter(function(item) identical(item$entry, resolved$entry), resolved$entries)
  if (length(entry) != 1L) stop("resolved specification lacks its entry")
  declared <- entry[[1L]]$specification$columns
  types <- setNames(vapply(declared, function(column) column$type, character(1L)),
    vapply(declared, function(column) column$name, character(1L)))
  missing <- setdiff(columns, names(types))
  if (length(missing)) stop("output columns without a declared type: ", paste(missing, collapse=", "))
  types[columns]
}
issue_rows <- function(frame) lapply(seq_len(nrow(frame)), function(index) list(
  phase=frame$phase[[index]], condition=frame$condition[[index]], rule=frame$rule[[index]],
  spec_paths=as.list(frame$spec_paths[[index]]), context=raw_json(frame$context[[index]])))
# How a run got its plan, from its private observations: compiled, refused or
# reused, or NULL when they name none. The measurements close the
# observations, so their plan is the last one named. Match and slice bytes. On
# Windows, R's default regular expressions count UTF-16 code units, two for a
# character outside the Basic Multilingual Plane, while a character slice
# counts it once: each one before the plan (an emoji in an output table or a
# saved file) would move the slice one character on.
observed_plan <- function(text) {
  found <- regmatches(text, gregexpr('"plan":"[a-z]+"', text, useBytes=TRUE))[[1L]]
  if (!length(found)) return(NULL)
  sub('^"plan":"([a-z]+)"$', "\\1", found[[length(found)]])
}
# A planned build runs one specification, with the project the plan names in
# its environment column. Generated metamorphic variants are not listed in
# their project, so project builds use the package's spec-with-project
# constructor, the one project$spec(name) returns for a listed specification.
open_spec <- function(case, specification, project) {
  path <- file.path(case, specification)
  if (is.null(project)) return(yamaa_spec(path))
  namespace <- asNamespace("yamaa")
  text <- get(".text", envir=namespace)
  get(".spec", envir=namespace)(text(path, "path"), text(file.path(case, project), "path"))
}
# Read every plan column as text. Otherwise read.delim() types a column that is
# blank on every row as logical NA (blank fields are missing in logical and
# numeric columns even with na.strings = character()), and nzchar(NA) is TRUE.
read_plan <- function(directory) read.delim(file.path(directory, "r.tsv"), colClasses = "character",
  check.names = FALSE, quote = "", na.strings = character(), fileEncoding = "UTF-8")
run_conformance <- function(args = commandArgs(trailingOnly = TRUE)) {
  if (length(args) != 4L) stop("usage: public_conformance.R PLAN FIXTURES OUTPUT SOURCE_REVISION")
  plan <- read_plan(args[[1L]])
  fixtures <- normalizePath(args[[2L]], mustWork = TRUE, winslash = "/")
  written <- args[[3L]]
  if (!grepl("^(/|[[:alpha:]]:)", written)) written <- file.path(getwd(), written)
  ancestor <- dirname(written)
  suffix <- basename(written)
  while (!dir.exists(ancestor)) {
    suffix <- file.path(basename(ancestor), suffix)
    ancestor <- dirname(ancestor)
  }
  output <- file.path(normalizePath(ancestor, mustWork=TRUE, winslash="/"), suffix)
  if (file.exists(output) || startsWith(paste0(output, "/"), paste0(fixtures, "/")))
    stop("output must be a fresh directory outside the fixture tree")
  dir.create(dirname(output), recursive=TRUE, showWarnings=FALSE)
  library(yamaa)
  package <- normalizePath(find.package("yamaa"), mustWork=TRUE, winslash="/")
  if (startsWith(paste0(package, "/"), paste0(dirname(fixtures), "/")))
    stop("public runner requires an installed package outside the fixture checkout")
  write_json(file.path(output, "run.json"), list(protocol="public-conformance/1", runtime="r",
    runtime_version=as.character(getRversion()), host_package_version=as.character(packageVersion("yamaa")),
    engine_version=yamaa:::.core_version(), source_revision=args[[4L]],
    platform=R.version$platform, package_form=Sys.getenv("PUBLIC_CONFORMANCE_PACKAGE_FORM", "installed"),
    evidence=paste0(Sys.getenv("GITHUB_SERVER_URL"), "/", Sys.getenv("GITHUB_REPOSITORY"), "/actions/runs/", Sys.getenv("GITHUB_RUN_ID")),
    table_encoding="str:utf8-hex,float:ieee754-be,int:decimal,date:epoch-days,datetime:epoch-seconds"))
  observe <- function(spec, built) {
    issues <- issue_rows(built$issues)
    output <- built$output
    types <- if (is.null(output)) NULL else declared_types(spec, names(output))
    # The run report is a compared table like the output, as in the Python
    # runner. Its check rows follow column declaration order (CHECK-23), so a
    # key-order variant compares them without their order.
    tables <- list(output=public_table(output, types),
      report=public_table(built$report, report_types))
    # A refused save signals yamaa_error; the record keeps the run's issues.
    saved <- if(length(issues)) FALSE else tryCatch({built$save(); TRUE}, yamaa_error=function(error) FALSE)
    list(issues=issue_rows(built$issues), tables=tables, saved=saved)
  }
  # The bytes of every regular file in a case; staged links are skipped.
  case_files <- function(case) {
    paths <- sort(list.files(case, recursive=TRUE, all.files=TRUE, no..=TRUE))
    paths <- paths[!nzchar(Sys.readlink(file.path(case, paths)))]
    setNames(lapply(paths, function(path) {
      file <- file.path(case, path)
      readBin(file, "raw", n=file.info(file)$size)
    }), paths)
  }
  plan_of <- function(built) observed_plan(yamaa:::.run_observations(built, include_measurements=TRUE))
  build_one <- function(case, specification, environment, repeat_run=FALSE) {
    spec <- open_spec(case, specification, environment)
    first <- spec$run()
    record <- c(list(specification=specification, environment=environment), observe(spec, first))
    if (repeat_run) {
      # The repeat pass (#2821): a second run of the same object reuses what
      # the first compiled and must equal it, saving the same bytes.
      record$plan <- plan_of(first)
      before <- case_files(case)
      second <- spec$run()
      again <- observe(spec, second)
      after <- case_files(case)
      paths <- sort(union(names(before), names(after)))
      again$plan <- plan_of(second)
      again$changed_files <- as.list(paths[!vapply(paths, function(path)
        identical(before[[path]], after[[path]]), logical(1L))])
      record[["repeat"]] <- again
    }
    record
  }
  for (name in unique(plan$example)) {
    all_rows <- plan[plan$example == name, , drop=FALSE]
    entries <- all_rows[!nzchar(all_rows$variant), , drop=FALSE]
    variant_rows <- all_rows[nzchar(all_rows$variant), , drop=FALSE]
    record <- list(example=name, builds=list(), error=NULL, variants=list())
    record <- tryCatch({
      if (!nzchar(entries$excluded[[1L]])) {
        case <- file.path(output, "work", name)
        move_inputs(file.path(args[[1L]], "r-inputs", name), case)
        for (index in seq_len(nrow(entries))) {
          entry <- entries[index, , drop=FALSE]
          environment <- if(nzchar(entry$environment[[1L]])) entry$environment[[1L]] else NULL
          record$builds[[length(record$builds)+1L]] <- build_one(case, entry$specification[[1L]],
            environment, repeat_run=TRUE)
        }
        variant_case <- case
        if (nrow(variant_rows)) {
          # Variants declare the same output paths as their entry, so build
          # them in a separate staged case: a variant's save() must not
          # overwrite the entry's saved files, whose bytes the comparator
          # checks against independent truth.
          variant_case <- file.path(output, "work", paste0(name, "__variants"))
          move_inputs(file.path(args[[1L]], "r-variant-inputs", name), variant_case)
        }
        for (index in seq_len(nrow(variant_rows))) {
          vrow <- variant_rows[index, , drop=FALSE]
          environment <- if(nzchar(vrow$environment[[1L]])) vrow$environment[[1L]] else NULL
          variant <- build_one(variant_case, vrow$specification[[1L]], environment)
          variant$kind <- vrow$variant[[1L]]
          variant$entry <- as.character(vrow$entry[[1L]])
          record$variants[[length(record$variants)+1L]] <- variant
        }
      }
      record
    }, error=function(error) { record$error <- paste0(class(error)[[1L]], ": ", conditionMessage(error)); record })
    write_json(file.path(output, "records", paste0(name, ".json")), record)
    cat(name, if(is.null(record$error)) "observed" else "error", "\n")
  }
  invisible(NULL)
}
if (sys.nframe() == 0L) run_conformance()
