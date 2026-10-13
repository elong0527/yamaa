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
move_inputs <- function(source, target) {
  if (!dir.exists(source)) stop("missing fresh R inputs; regenerate the conformance plan: ", source)
  dir.create(dirname(target), recursive=TRUE, showWarnings=FALSE)
  if (!file.rename(source, target)) stop("could not move staged R inputs to execution work: ", source)
}
public_table <- function(frame) {
  if (is.null(frame)) return(NULL)
  kinds <- vapply(frame, function(column) {
    if (inherits(column, "yamaa_int64_vector")) "int"
    else if (inherits(column, "Date")) "date"
    else if (inherits(column, "POSIXct")) "datetime"
    else if (is.character(column) || inherits(column, "yamaa_utf8_vector")) "str"
    else if (is.double(column)) "float"
    else stop("unexpected public column type: ", paste(class(column), collapse=","))
  }, character(1L))
  cell <- function(column, index, kind) {
    if (inherits(column, "yamaa_utf8_vector")) {
      value <- unclass(column)[[index]]
      return(if(is.null(value)) NULL else hex(value))
    }
    value <- unclass(column)[[index]]
    if (is.na(value)) return(NULL)
    switch(kind, str=hex(charToRaw(enc2utf8(value))), int=value,
      float=hex(writeBin(value, raw(), size=8L, endian="big")),
      date=sprintf("%.0f", value), datetime=sprintf("%.0f", value))
  }
  list(columns=lapply(seq_along(frame), function(index) as.list(c(names(frame)[[index]], kinds[[index]]))),
    rows=lapply(seq_len(nrow(frame)), function(row) lapply(seq_along(frame), function(column) cell(frame[[column]], row, kinds[[column]]))))
}
issue_rows <- function(frame) lapply(seq_len(nrow(frame)), function(index) list(
  phase=frame$phase[[index]], condition=frame$condition[[index]], requirement=frame$requirement[[index]],
  spec_paths=as.list(frame$spec_paths[[index]]), context=raw_json(frame$context[[index]])))
run_conformance <- function(args = commandArgs(trailingOnly = TRUE)) {
  if (length(args) != 4L) stop("usage: public_conformance.R PLAN FIXTURES OUTPUT SOURCE_REVISION")
  plan <- read.delim(file.path(args[[1L]], "r.tsv"), stringsAsFactors = FALSE,
    check.names = FALSE, quote = "", na.strings = character(), fileEncoding = "UTF-8")
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
  for (name in unique(plan$example)) {
    entries <- plan[plan$example == name, , drop=FALSE]
    record <- list(example=name, builds=list(), error=NULL)
    record <- tryCatch({
      if (!nzchar(entries$excluded[[1L]])) {
        case <- file.path(output, "work", name)
        move_inputs(file.path(args[[1L]], "r-inputs", name), case)
        for (index in seq_len(nrow(entries))) {
          entry <- entries[index, , drop=FALSE]
          environment <- if(nzchar(entry$environment[[1L]])) entry$environment[[1L]] else NULL
          built <- yamaa_domain(file.path(case, entry$specification[[1L]]),
            environment=if(is.null(environment)) NULL else file.path(case, environment))
          issues <- issue_rows(built$issues)
          tables <- list(output=public_table(built$output), verification_log=public_table(built$verification_log),
            warning_log=public_table(built$warning_log))
          saved <- if(length(issues)) FALSE else built$save()
          issues <- issue_rows(built$issues)
          record$builds[[length(record$builds)+1L]] <- list(specification=entry$specification[[1L]],
            environment=environment, issues=issues, tables=tables, saved=saved)
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
