# rows.R -- row construction (rules/execution/rows.md).
#
# build_rows(spec, ctx0) -> ctx with:
#   df: data.frame of constructed rows (row-derived columns only, declaration
#       order across templates by first appearance; other templates' rows NA)
#   driver_ds: character per row, driver_rec: list of int vectors per row
#   row_coltypes: named char of row-derived column types
# Rows are built before any column derivation runs.

build_rows <- function(spec, ctx0) {
  keys <- spec$keys
  if (is.null(keys)) keys <- character(0)
  if (!is.null(spec$rows) && !is.null(spec$filter))
    yamaa_error("invalid_spec", "filter: cannot combine with rows:")
  if (!is.null(spec$rows)) {
    build_template_rows(spec, ctx0, keys)
  } else {
    build_base_rows(spec, ctx0, keys)
  }
}

default_dataset <- function(spec) {
  if (!is.null(spec$base)) return(spec$base)
  # spec$inputs is a named list; spec$input (singular) is legacy
  inp <- spec$inputs
  if (is.null(inp)) inp <- spec$input
  ids <- names(inp)
  if (length(ids) == 1) return(ids)
  yamaa_error("invalid_spec", "no rows:, no base:, and not a single input dataset")
}

build_base_rows <- function(spec, ctx0, keys) {
  ds <- default_dataset(spec)
  ydf <- ctx0$inputs[[ds]]
  recs <- seq_len(nrow(ydf))
  recs <- apply_record_filter(ctx0, ds, recs, spec$filter)
  colspecs <- list()
  for (c in spec$columns) colspecs[[c$name]] <- c
  # REQ-0042: no `rows:` -> the output row set is the distinct combination of
  # keys over the input records, in first-appearance order. The key table is
  # standalone: column derivations read the records behind each key
  # combination (via driver_rec), which decide values but not row count.
  if (length(keys) == 0) {
    groups <- lapply(recs, function(r) as.integer(r))
  } else {
    for (k in keys) if (is.null(colspecs[[k]]))
      yamaa_error("invalid_spec", paste0("key column not declared: ", k))
    # key values over all input records at once: a temporary single-record
    # driver context lets key derivations (incl. window keys like a sequence
    # number) evaluate vectorized. Keys evaluate in column-declaration order
    # so a window key's unqualified group_by resolves against earlier keys;
    # REQ-0074: keys cannot depend on non-key output columns, so col holds
    # only keys and other references fail here.
    ctxk <- ctx0
    ctxk$n <- length(recs)
    ctxk$driver_ds <- rep(ds, length(recs))
    ctxk$driver_rec <- lapply(recs, function(r) as.integer(r))
    ctxk$phase <- "key"
    ctxk$col <- list()
    decl_order <- match(keys, vapply(spec$columns, function(c) c$name, character(1)))
    keyvecs <- list()
    for (k in keys[order(decl_order)]) {
      tvv <- eval_expression(colspecs[[k]]$derivation, ctxk)
      if (tv_len(tvv) != length(recs))
        yamaa_error("invalid_spec",
          paste0("key column ", k, " derivation must yield one value per record"))
      ctxk$col[[k]] <- tvv
      keyvecs[[k]] <- vapply(seq_along(recs),
        function(j) canon_key_text(tvv$t, tvv$v[j]), character(1))
    }
    seen <- new.env(hash = TRUE, parent = emptyenv())
    groups <- list()
    for (j in seq_along(recs)) {
      gk <- paste(vapply(keys, function(k) keyvecs[[k]][j], character(1)),
        collapse = "\x1f")
      if (!exists(gk, envir = seen, inherits = FALSE)) {
        assign(gk, length(groups) + 1L, envir = seen)
        groups[[length(groups) + 1L]] <- as.integer(recs[j])
      } else {
        gi <- get(gk, envir = seen, inherits = FALSE)
        groups[[gi]] <- c(groups[[gi]], as.integer(recs[j]))
      }
    }
  }
  n <- length(groups)
  ctx <- ctx0
  ctx$n <- n
  ctx$df <- empty_df(n)
  ctx$driver_ds <- rep(ds, n)
  ctx$driver_rec <- groups
  ctx$row_coltypes <- character(0)
  ctx$phase <- "column"
  ctx
}

empty_df <- function(n) {
  df <- data.frame(row.names = seq_len(n), check.names = FALSE)
  df
}

build_template_rows <- function(spec, ctx0, keys) {
  colspecs <- list()
  for (c in spec$columns) colspecs[[c$name]] <- c
  ctx0$colspecs <- colspecs
  col_data <- list()   # name -> list(tv per template block)
  col_types <- character(0)
  driver_ds <- character(0)
  driver_rec <- list()
  # default dataset only needed if some template lacks one
  need_default <- any(vapply(spec$rows, function(t) is.null(t$dataset), logical(1)))
  tmpl_ds_default <- if (need_default) default_dataset(spec) else NULL

  for (t in spec$rows) {
    ds <- if (!is.null(t$dataset)) t$dataset else tmpl_ds_default
    if (!ds %in% names(ctx0$inputs)) yamaa_error("unknown_field", paste0("row dataset: ", ds))
    # REQ-0326: a window depending on another window's result fails validation
    validate_row_window_deps(t)
    ydf <- ctx0$inputs[[ds]]
    recs <- seq_len(nrow(ydf))
    if (!is.null(t$group_by)) {
      blk <- build_grouped_block(t, ds, ydf, recs, ctx0, keys, spec)
    } else {
      # ungrouped: filter input records first
      recs <- apply_record_filter(ctx0, ds, recs, t$filter)
      blk <- build_record_block(t, ds, ydf, recs, ctx0, keys, spec)
    }
    # merge block columns
    for (nm in names(blk$cols)) {
      tvb <- blk$cols[[nm]]
      if (!nm %in% names(col_data)) {
        col_data[[nm]] <- tvb$v
        col_types[[nm]] <- tvb$t
      } else {
        if (col_types[[nm]] != tvb$t)
          yamaa_error("incompatible_input_type",
            paste0("row column ", nm, " has conflicting types across templates"))
        col_data[[nm]] <- c(col_data[[nm]], tvb$v)
      }
    }
    # templates that don't derive a column: NA-fill for this block's rows
    nb <- blk$n
    if (nb > 0) {
      for (nm in setdiff(names(col_data), names(blk$cols))) {
        na <- tv_na(col_types[[nm]], nb)$v
        col_data[[nm]] <- c(col_data[[nm]], na)
      }
    }
    driver_ds <- c(driver_ds, blk$driver_ds)
    driver_rec <- c(driver_rec, blk$driver_rec)
  }

  n <- length(driver_ds)
  df <- as.data.frame(col_data, stringsAsFactors = FALSE, check.names = FALSE)
  attr(df, "coltypes") <- col_types
  ctx <- ctx0
  ctx$n <- n
  ctx$df <- df
  ctx$col <- lapply(names(col_data), function(nm) tv(col_data[[nm]], col_types[[nm]]))
  names(ctx$col) <- names(col_data)
  ctx$coltypes <- col_types
  ctx$driver_ds <- driver_ds
  ctx$driver_rec <- driver_rec
  ctx$row_coltypes <- col_types
  ctx$phase <- "column"
  ctx
}

# record-driven template block ---------------------------------------------
build_record_block <- function(t, ds, ydf, recs, ctx0, keys, spec) {
  n <- length(recs)
  ctx <- ctx0
  ctx$n <- n
  ctx$phase <- "row-record"
  ctx$tmpl_ds <- ds
  ctx$driver_ds <- rep(ds, n)
  ctx$driver_rec <- lapply(recs, function(r) as.integer(r))
  ctx$row_keys <- input_record_keys(ctx0, ds, ydf, keys, recs)
  ctx$col <- list()
  ctx$coltypes <- character(0)
  ctx <- eval_template_derivations(t, ctx)
  cols <- ctx$col
  list(n = n, cols = cols, driver_ds = ctx$driver_ds, driver_rec = ctx$driver_rec)
}

# grouped template block -----------------------------------------------------
build_grouped_block <- function(t, ds, ydf, recs, ctx0, keys, spec) {
  gb <- t$group_by  # qualified names
  gb_cols <- vapply(gb, function(g) {
    s <- split_qual(g)
    if (s$q != ds) yamaa_error("invalid_spec", "group_by must name the template's dataset")
    s$v
  }, character(1))
  cts <- attr(ydf, "coltypes")
  kv <- vapply(recs, function(r)
    paste(vapply(gb_cols, function(k) canon_key_text(cts[[k]], ydf[[k]][r]), character(1)),
      collapse = "\x1f"), character(1))
  # missing == missing for grouping (REQ-0184): canon_key_text already maps NA
  groups <- split(recs, factor(kv, levels = unique(kv)))
  ng <- length(groups)
  # group key values, keyed by unqualified key name
  gkey_vals <- list()
  for (j in seq_along(gb_cols)) {
    k <- gb_cols[j]; tk <- cts[[k]]
    firsts <- vapply(groups, function(g) g[1], integer(1))
    gkey_vals[[k]] <- tv(ydf[[k]][firsts], tk)
  }
  ctx <- ctx0
  ctx$n <- ng
  ctx$phase <- "row-grouped"
  ctx$tmpl_ds <- ds
  ctx$driver_ds <- rep(ds, ng)
  ctx$driver_rec <- lapply(groups, function(g) as.integer(g))
  ctx$group_rec <- ctx$driver_rec
  ctx$row_keys <- gkey_vals
  ctx$col <- list()
  ctx$coltypes <- character(0)
  ctx <- eval_template_derivations(t, ctx)
  # grouped filter over the candidate's completed unqualified columns
  keep <- rep(TRUE, ng)
  if (!is.null(t$filter)) {
    resolver <- list(resolve = function(name) {
      s <- split_qual(name)
      if (!is.null(s$q)) yamaa_error("invalid_spec", "grouped row filter takes no qualified names")
      if (!s$v %in% names(ctx$col)) yamaa_error("unknown_field", s$v)
      ctx$col[[s$v]]
    }, n = ng)
    f <- eval_pred(parse_predicate_text(t$filter), resolver)
    keep <- !is.na(f) & f
  }
  cols <- lapply(ctx$col, function(c) tv(c$v[keep], c$t))
  list(n = sum(keep), cols = cols,
    driver_ds = ctx$driver_ds[keep], driver_rec = ctx$driver_rec[keep])
}

# input records' key field values, for row-phase implicit joins
input_record_keys <- function(ctx0, ds, ydf, keys, recs) {
  cts <- attr(ydf, "coltypes")
  out <- list()
  for (k in keys) {
    if (k %in% names(ydf)) out[[k]] <- tv(ydf[[k]][recs], cts[[k]])
  }
  out
}

# one derivation through stages 1-3: evaluate, convert to the declared type,
# REQ-0344/0359: omitted `missing:` makes conversion failure fatal; explicit
eval_derivation <- function(deriv, declared_type, colname, ctx) {
  cf <- NULL; cf_present <- FALSE
  expr <- deriv
  if (is.list(deriv) && !is.null(names(deriv)) && "value" %in% names(deriv)) {
    cf_present <- "missing" %in% names(deriv)
    cf <- deriv[["missing"]]
    expr <- deriv$value
  }
  tvv <- eval_expression(expr, ctx)
  apply_declared_type(tvv, declared_type, cf, cf_present, colname)
}

# top-level expression kind, unwrapping the handled `value` wrapper
deriv_kind <- function(deriv) {
  if (is.character(deriv)) return("source")
  if (is.list(deriv) && !is.null(names(deriv))) {
    if ("value" %in% names(deriv)) return(deriv_kind(deriv$value))
    return(names(deriv)[1])
  }
  NULL
}

WINDOW_KINDS <- c("row_number", "rank", "row_value", "previous_non_missing",
  "baseline_flag")

# record a window-derived column so later windows refuse to read it (REQ-0327)
note_window_col <- function(ctx, nm, deriv) {
  if (deriv_kind(deriv) %in% WINDOW_KINDS)
    ctx$from_window <- c(ctx$from_window, nm)
  ctx
}

# ---- row-window dependency validation (REQ-0326) ---------------------------
# A window expression used during row construction partitions the rows its
validate_row_window_deps <- function(t) {
  derivs <- t$derivations
  if (is.null(derivs) || length(derivs) == 0) return(invisible(NULL))
  nms <- names(derivs)
  is_win <- vapply(nms,
    function(nm) deriv_kind(derivs[[nm]]) %in% WINDOW_KINDS, logical(1))
  if (!any(is_win)) return(invisible(NULL))
  refs <- lapply(nms, function(nm) deriv_refs(derivs[[nm]], nms))
  names(refs) <- nms
  # transitive window reachability, memoized; a revisit means a dependency
  # cycle, which the fixed-point loop reports as unresolved_reference later
  memo <- new.env(hash = TRUE, parent = emptyenv())
  touches_window <- function(nm, seen) {
    if (!nm %in% nms) return(FALSE)
    if (exists(nm, envir = memo, inherits = FALSE)) return(get(nm, envir = memo))
    if (nm %in% seen) return(FALSE)
    if (is_win[[nm]]) { assign(nm, TRUE, envir = memo); return(TRUE) }
    r <- any(vapply(refs[[nm]],
      function(d) touches_window(d, c(seen, nm)), logical(1)))
    assign(nm, r, envir = memo)
    r
  }
  for (w in nms[is_win]) {
    bad <- refs[[w]][vapply(refs[[w]],
      function(d) touches_window(d, character(0)), logical(1))]
    if (length(bad) > 0)
      yamaa_error("window_on_window_result",
        paste0("row window '", w, "' depends on window-derived value via: ",
          paste(bad, collapse = ", ")))
  }
  invisible(NULL)
}

# fixed-point derivation evaluation; defers only on unknown_field.
# values (REQ-0217).
eval_template_derivations <- function(t, ctx) {
  derivs <- t$derivations
  if (is.null(derivs)) derivs <- list()
  for (nm in names(derivs)) {
    if (!nm %in% names(ctx$colspecs))
      yamaa_error("invalid_spec", paste0("row derivation names undeclared column: ", nm))
  }
  remaining <- names(derivs)
  guard <- 0L
  while (length(remaining) > 0) {
    guard <- guard + 1L
    if (guard > 10000) yamaa_error("internal", "derivation loop guard")
    progress <- FALSE
    for (nm in remaining) {
      r <- tryCatch(
        list(ok = TRUE,
          tv = eval_derivation(derivs[[nm]], ctx$colspecs[[nm]]$type, nm, ctx)),
        yamaa_error = function(e) {
          if (attr(e, "yamaa_condition") == "unknown_field") list(ok = FALSE)
          else stop(e)
        })
      if (r$ok) {
        ctx$col[[nm]] <- r$tv
        ctx$coltypes[[nm]] <- r$tv$t
        ctx <- note_window_col(ctx, nm, derivs[[nm]])
        remaining <- setdiff(remaining, nm)
        progress <- TRUE
      }
    }
    if (!progress)
      yamaa_error("unresolved_reference",
        paste0("row derivations have a cycle or unknown reference: ",
          paste(remaining, collapse = ", ")))
  }
  ctx
}
