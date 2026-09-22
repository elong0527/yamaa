# columns.R -- column derivation phase (rules/execution/lifecycle.md).
#
# derive_columns(spec, ctx): for each declared column in declaration order,
# evaluate its derivation (stages 1-3) unless row construction already did,
# then run stage 4 verifications. Updates ctx$col / ctx$df.

derive_columns <- function(spec, ctx) {
  ctx$phase <- "column"
  colnames <- vapply(spec$columns, function(c) c$name, character(1))
  # REQ-0048/0071/0072: infer dependencies, validate declaration order,
  # then execute in topo order (stable by declaration).
  deps <- lapply(spec$columns, function(c) deriv_refs(c$derivation, colnames))
  names(deps) <- colnames
  pos <- setNames(seq_along(colnames), colnames)
  # detect cycles first: a cycle is dependency_cycle, not forward_reference
  if (has_cycle(colnames, deps))
    yamaa_error("dependency_cycle", "column derivations contain a dependency cycle")
  for (nm in colnames) {
    for (d in deps[[nm]]) {
      if (pos[[d]] > pos[[nm]])
        yamaa_error("forward_reference",
          paste0("column ", nm, " references later-declared column ", d,
            " (REQ-0071)"))
    }
  }
  # implicit key dependencies for execution only (REQ-0150 inference):
  # a qualified aggregate/lookup without explicit key material joins on the
  # applicable output keys, which must be derived first. Not named, so no
  # REQ-0071 validation.
  keys <- spec$keys
  if (is.null(keys)) keys <- character(0)
  ideps <- lapply(spec$columns, function(c)
    if (deriv_needs_keys(c$derivation)) intersect(keys, colnames) else character(0))
  names(ideps) <- colnames
  order <- topo_order(colnames, deps, ideps, pos)
  for (nm in order) {
    c <- spec$columns[[pos[[nm]]]]
    if (nm %in% names(ctx$col)) next  # row-derived: stages 1-3 done
    tvv <- eval_derivation(c$derivation, c$type, nm, ctx)
    if (tv_len(tvv) != ctx$n)
      yamaa_error("invalid_spec",
        paste0("column ", nm, " derivation returned ", tv_len(tvv),
          " values for ", ctx$n, " rows"))
    ctx$col[[nm]] <- tvv
    ctx$coltypes[[nm]] <- tvv$t
    ctx <- note_window_col(ctx, nm, c$derivation)
  }
  # materialize the data.frame in declaration order
  df <- as.data.frame(
    lapply(spec$columns, function(c) ctx$col[[c$name]]$v),
    stringsAsFactors = FALSE, check.names = FALSE)
  names(df) <- vapply(spec$columns, function(c) c$name, character(1))
  attr(df, "coltypes") <- vapply(spec$columns,
    function(c) ctx$col[[c$name]]$t, character(1))
  ctx$df <- df
  # stage 4: column verifications
  for (c in spec$columns) {
    if (!is.null(c$verifications)) run_column_verifications(c, ctx)
  }
  ctx
}

# ---- dependency inference (REQ-0048..REQ-0056) -------------------------------
# unqualified output-column references in a parsed expression/predicate AST.
# All three grammars use list(kind="id", qualifier=, name=).
ast_col_refs <- function(node, colnames) {
  refs <- character(0)
  walk <- function(x) {
    if (!is.list(x)) return()
    if (identical(x$kind, "id") && is.null(x$qualifier) && x$name %in% colnames)
      refs <<- c(refs, x$name)
    for (e in x) walk(e)
  }
  walk(node)
  unique(refs)
}

bare_col_ref <- function(s, colnames) {
  q <- split_qual(s)
  if (is.null(q$q) && q$v %in% colnames) q$v else character(0)
}

# explicit unqualified output-column references in a derivation.
deriv_refs <- function(deriv, colnames) {
  refs <- character(0)
  walk_string <- function(x, key = NULL, parent = NULL) {
      # text parses are best-effort here; real parse errors surface at eval
      if (!is.null(key) && key %in% c("when", "then", "filter")) {
        node <- tryCatch(parse_predicate_text(x), error = function(e) NULL)
        if (!is.null(node)) refs <<- c(refs, ast_col_refs(node, colnames))
        return()
      }
      if (identical(key, "expr") && identical(parent, "compute")) {
        node <- tryCatch(parse_compute_text(x), error = function(e) NULL)
        if (!is.null(node)) refs <<- c(refs, ast_col_refs(node, colnames))
        return()
      }
      if (identical(key, "aggregate") || identical(parent, "aggregate")) {
        node <- tryCatch(parse_aggregate_text(x), error = function(e) NULL)
        if (!is.null(node)) refs <<- c(refs, ast_col_refs(node, colnames))
        return()
      }
      if (!is.null(key) && key %in% c("source", "sources", "variable", "value", "date",
          "key_base", "not_before", "group_by", "order_by")) {
        refs <<- c(refs, bare_col_ref(x, colnames))
        return()
      }
      # REQ-0456: str_template placeholders are column dependencies, parsed
      # best-effort here (real parse errors surface at eval)
      if (identical(parent, "str_template")) {
        parts <- tryCatch(parse_string_template(x), error = function(e) NULL)
        if (!is.null(parts))
          refs <<- c(refs, unlist(lapply(template_placeholders(parts),
            function(p) bare_col_ref(p, colnames))))
        return()
      }
      if (is.null(key)) {
        refs <<- c(refs, bare_col_ref(x, colnames))
        return()
      }
      return()  # literal text under any other key
  }
  walk <- function(x, key = NULL, parent = NULL) {
    if (is.character(x)) {
      # character vectors (e.g. a multi-name group_by list) contribute one
      # reference per element; previously only length-1 vectors were seen
      for (s in x) walk_string(s, key, parent)
      return()
    }
    if (!is.list(x)) return()
    # unwrap the handled `value` wrapper
    if (!is.null(names(x)) && "value" %in% names(x)) {
      walk(x$value, key, parent)
      return()
    }
    nm <- names(x)
    if (is.null(nm)) {
      for (e in x) walk(e, NULL, parent)
      return()
    }
    for (i in seq_along(x)) {
      k <- nm[i]
      pk <- if (k %in% EXPR_KINDS) k else parent
      walk(x[[i]], if (nzchar(k)) k else NULL, pk)
    }
  }
  walk(deriv, NULL, NULL)
  unique(refs)
}

# TRUE when the derivation joins on inferred output keys: a qualified
# aggregate, or a lookup, without explicit key material (REQ-0050/0150).
deriv_needs_keys <- function(deriv) {
  found <- FALSE
  walk <- function(x) {
    if (found) return()
    if (is.character(x) && length(x) == 1) return()
    if (!is.list(x)) return()
    if (!is.null(names(x)) && "value" %in% names(x)) {
      walk(x$value)
      return()
    }
    nm <- names(x)
    if (!is.null(nm) && length(x) == 1) {
      k <- nm[1]; p <- x[[1]]
      if (k == "aggregate") {
        payload <- if (is.character(p)) list(expr = p) else p
        node <- tryCatch(parse_aggregate_text(payload$expr),
          error = function(e) NULL)
        quals <- if (is.null(node)) character(0) else collect_qualifiers(node)
        if (length(quals) > 0 &&
            (is.null(payload$group_by) || is.null(payload$key))) {
          found <<- TRUE
          return()
        }
      }
      if (k == "lookup" && is.list(p) &&
          (is.null(p$key) || is.null(p$key_base))) {
        found <<- TRUE
        return()
      }
    }
    for (e in x) walk(e)
  }
  walk(deriv)
  found
}

# Kahn's algorithm, declaration-order tiebreak. deps: explicit refs (already
# REQ-0071 validated, so no forward edges); ideps: implicit key edges.
has_cycle <- function(colnames, deps) {
  # DFS cycle detection on the explicit dependency graph
  state <- setNames(rep(0L, length(colnames)), colnames)  # 0=unvisited,1=in-stack,2=done
  visit <- function(nm) {
    state[[nm]] <<- 1L
    for (d in deps[[nm]]) {
      if (d %in% colnames) {
        if (state[[d]] == 1L) return(TRUE)
        if (state[[d]] == 0L && visit(d)) return(TRUE)
      }
    }
    state[[nm]] <<- 2L
    FALSE
  }
  for (nm in colnames) {
    if (state[[nm]] == 0L && visit(nm)) return(TRUE)
  }
  FALSE
}

topo_order <- function(colnames, deps, ideps, pos) {
  all_deps <- lapply(colnames, function(nm)
    intersect(c(deps[[nm]], ideps[[nm]]), colnames))
  names(all_deps) <- colnames
  # explicit deps are REQ-0071-validated backward; implicit key edges may run
  # forward (keys declared later) and still force the keys first. Self edges
  # surface as cycles below.
  indeg <- vapply(colnames, function(nm) length(all_deps[[nm]]), integer(1))
  ready <- colnames[indeg == 0]
  out <- character(0)
  while (length(ready) > 0) {
    # declaration order among ready
    ready <- ready[order(pos[ready])]
    nm <- ready[1]
    ready <- ready[-1]
    out <- c(out, nm)
    for (m in colnames) {
      if (nm %in% all_deps[[m]]) {
        indeg[[m]] <- indeg[[m]] - 1
        if (indeg[[m]] == 0) ready <- c(ready, m)
      }
    }
  }
  if (length(out) != length(colnames)) {
    cyc <- setdiff(colnames, out)
    yamaa_error("dependency_cycle",
      paste0("column dependency cycle: ", paste(cyc, collapse = " -> "),
        " (REQ-0072)"))
  }
  out
}
