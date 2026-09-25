# publish.R -- artifact publication (rules/storage/publication.md).
#
# A run publishes up to three artifacts: the verification report, the
# violation log, and the primary output. REQ-0757: every artifact is
# rendered to a same-directory temporary regular file first, then
# published report -> violation log -> primary (REQ-0757/1181); a failed
# run publishes the report alone (REQ-1181). If any render fails, no
# published target is altered; replaces then proceed in order, and a
# failed replace stops the sequence.

# ---- publication plan ---------------------------------------------------
publish_run <- function(spec, ctx, out_path, failed) {
  jobs <- list()
  vrep <- spec$output$verification_log
  if (!is.null(vrep) && nzchar(vrep))
    jobs[[length(jobs) + 1L]] <- list(target = file.path(dirname(out_path), vrep),
      write = function(p) write_typed_artifact(
        build_verification_log(spec, ctx), vreport_coltypes(), p))
  if (!failed && spec_has_warning(spec))
    jobs[[length(jobs) + 1L]] <- list(target = file.path(dirname(out_path),
        spec$output$warning_log),
      write = function(p) write_typed_artifact(
        build_warning_log(spec, ctx), vlog_coltypes(), p))
  if (!failed)
    jobs[[length(jobs) + 1L]] <- list(target = out_path,
      write = function(p) {
        fr <- build_primary_frame(spec, ctx)
        write_typed_artifact(fr$df, fr$coltypes, p, spec$output$decimals)
      })
  # render every artifact before touching any target (REQ-0757)
  temps <- character(length(jobs))
  for (i in seq_along(jobs)) {
    temps[i] <- tryCatch(publish_file_atomic(jobs[[i]]$target, jobs[[i]]$write),
      error = function(e) { unlink(temps[nzchar(temps)]); stop(e) })
  }
  # replace targets in order: report, violation log, primary
  for (i in seq_along(jobs)) {
    ok <- tryCatch(file.rename(temps[i], jobs[[i]]$target),
      error = function(e) FALSE)
    if (!isTRUE(ok)) {
      unlink(temps[i])
      yamaa_error("publication_failed",
        paste0("could not replace ", jobs[[i]]$target))
    }
  }
  invisible(out_path)
}

# Render one artifact to a same-directory temporary regular file; the
# keeps the target's extension so the profile dispatch sees it (REQ-0716).
publish_file_atomic <- function(target, write_fn) {
  fe <- tools::file_ext(target)
  tmp <- tempfile(pattern = "yamaa-pub-", tmpdir = dirname(target),
    fileext = if (nzchar(fe)) paste0(".", fe) else "")
  err <- tryCatch({ write_fn(tmp); NULL }, error = function(e) e)
  if (!is.null(err)) {
    unlink(tmp)
    if (inherits(err, "yamaa_error")) stop(err)
    yamaa_error("publication_failed",
      paste0("could not render ", target, ": ", conditionMessage(err)))
  }
  tmp
}

# ---- primary output -----------------------------------------------------
# build_primary_frame returns list(df, coltypes): typed storage vectors,
# already ordered and projected to output.columns.
build_primary_frame <- function(spec, ctx) {
  n <- ctx$n
  cols <- spec$output$columns
  if (is.null(cols)) yamaa_error("invalid_spec", "output.columns is required")
  # REQ-0233: keys must be published columns (not internal)
  keys <- spec$keys
  if (!is.null(keys)) {
    for (k in keys) {
      if (!k %in% cols)
        yamaa_error("internal_column_in_keys", paste0("key not in output.columns: ", k))
    }
  }
  for (cn in cols) {
    if (!cn %in% names(ctx$col))
      yamaa_error("unknown_field", paste0("output column: ", cn))
  }
  idx <- seq_len(n)
  order_spec <- spec$output$order_by
  if (is.null(order_spec)) order_spec <- spec$order
  if (!is.null(order_spec)) {
    # REQ-0237: repeated ordering terms fail
    terms <- vapply(order_spec, function(t) {
      if (is.character(t)) t
      else if (!is.null(t$column)) t$column
      else t$variable
    }, character(1))
    dup <- terms[duplicated(terms)]
    if (length(dup) > 0)
      yamaa_error("duplicate_order_term", paste0("repeated order term: ", dup[1]))
    # REQ-0236: an order_by term must name a declared column (not
    # necessarily an output column)
    for (t in terms) {
      if (!t %in% names(ctx$col))
        yamaa_error("undeclared_column", paste0("order_by term not a declared column: ", t))
    }
    idx <- order_output_rows(order_spec, ctx, idx)
  }
  df <- as.data.frame(
    lapply(cols, function(cn) ctx$col[[cn]]$v[idx]),
    stringsAsFactors = FALSE, check.names = FALSE)
  names(df) <- cols
  # REQ-0240: output keys must be unique
  keys <- spec$keys
  if (!is.null(keys) && all(keys %in% cols)) {
    kdf <- df[keys]
    if (anyDuplicated(kdf) > 0)
      yamaa_error("duplicate_key", "output contains duplicate key combinations")
  }
  coltypes <- vapply(cols, function(cn) ctx$col[[cn]]$t, character(1), USE.NAMES = FALSE)
  names(coltypes) <- cols
  list(df = df, coltypes = coltypes)
}

order_output_rows <- function(order_spec, ctx, idx) {
  # order: list of {variable, direction, nulls}
  keys <- lapply(order_spec, function(tm) {
    var <- if (is.character(tm)) tm else tm$variable
    dir <- if (is.character(tm) || is.null(tm$direction)) "asc" else tm$direction
    nulls <- if (is.character(tm) || is.null(tm$nulls)) "last" else tm$nulls
    if (!var %in% names(ctx$col)) yamaa_error("unknown_field", var)
    c <- ctx$col[[var]]
    list(v = c$v, t = c$t, dir = dir, nulls = nulls)
  })
  cmp <- make_row_cmp(keys)
  merge_sort_idx(idx, cmp)
}

# ---- violation log --------------------------------------------------------
# REQ-0392: the governed violation-log columns and types.
vlog_coltypes <- function() c(LOG_VERSION = "str", ARTIFACT = "str",
  SEVERITY = "str", CONDITION = "str", REQUIREMENT = "str", SPEC_PATH = "str",
  VERIFICATION_ID = "str", FAILURE_COUNT = "int", OFFENDING_KEYS = "str",
  DETAILS = "str")

# REQ-0391/0392/0393/0396: the warning violation log, one row per warning
# violation, written even when zero violations occurred.
build_warning_log <- function(spec, ctx) {
  entries <- ctx$warn_env$entries
  ne <- length(entries)
  df <- data.frame(
    LOG_VERSION = rep("1.0", ne),
    ARTIFACT = rep(spec$output$path, ne),
    SEVERITY = rep("warning", ne),
    CONDITION = if (ne) vapply(entries, function(e) e$condition, character(1)) else character(0),
    REQUIREMENT = if (ne) vapply(entries, function(e) e$requirement, character(1)) else character(0),
    SPEC_PATH = if (ne) vapply(entries, function(e) e$spec_path, character(1)) else character(0),
    VERIFICATION_ID = if (ne) vapply(entries, function(e)
      if (is.null(e$verification_id)) NA_character_ else as.character(e$verification_id),
      character(1)) else character(0),
    FAILURE_COUNT = if (ne) vapply(entries, function(e) as.integer(e$failure_count), integer(1)) else integer(0),
    OFFENDING_KEYS = if (ne) vapply(entries, function(e) e$offending_keys, character(1)) else character(0),
    DETAILS = if (ne) vapply(entries, function(e) e$details, character(1)) else character(0),
    stringsAsFactors = FALSE, check.names = FALSE)
  df
}

# ---- verification report ----------------------------------------------------
# REQ-1173..1181: one row per evaluated verification, in execution order.
vreport_coltypes <- function() c(REPORT_VERSION = "str", ARTIFACT = "str",
  SPEC_PATH = "str", VERIFICATION_ID = "str", CHECK = "str", TARGET = "str",
  REQUIREMENT = "str", SEVERITY = "str", OUTCOME = "str", CONDITION = "str",
  EVALUATED_COUNT = "int", FAILURE_COUNT = "int", DETAILS = "str")

build_verification_log <- function(spec, ctx) {
  rows <- ctx$check_env$rows
  ne <- length(rows)
  df <- data.frame(
    REPORT_VERSION = rep("1.0", ne),
    ARTIFACT = rep(spec$output$path, ne),
    SPEC_PATH = if (ne) vapply(rows, function(r) r$spec_path, character(1)) else character(0),
    VERIFICATION_ID = if (ne) vapply(rows, function(r) r$verification_id, character(1)) else character(0),
    CHECK = if (ne) vapply(rows, function(r) r$check, character(1)) else character(0),
    TARGET = if (ne) vapply(rows, function(r) r$target, character(1)) else character(0),
    REQUIREMENT = if (ne) vapply(rows, function(r) r$requirement, character(1)) else character(0),
    SEVERITY = if (ne) vapply(rows, function(r) r$severity, character(1)) else character(0),
    OUTCOME = if (ne) vapply(rows, function(r) r$outcome, character(1)) else character(0),
    CONDITION = if (ne) vapply(rows, function(r) r$condition, character(1)) else character(0),
    EVALUATED_COUNT = if (ne) vapply(rows, function(r) as.integer(r$evaluated_count), integer(1)) else integer(0),
    FAILURE_COUNT = if (ne) vapply(rows, function(r) as.integer(r$failure_count), integer(1)) else integer(0),
    DETAILS = if (ne) vapply(rows, function(r) r$details, character(1)) else character(0),
    stringsAsFactors = FALSE, check.names = FALSE)
  df
}
