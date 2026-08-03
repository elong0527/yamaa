# Execute a specification, R implementation.
#
# The counterpart of tests/python/runner.py. Same phases, same rules, and it must
# serialize to byte-identical output.

ODM_CONTEXT <- c("StudyOID", "SubjectKey", "StudyEventOID",
                 "StudyEventRepeatKey", "ItemGroupOID", "ItemGroupRepeatKey")

HANDLED_STAGES <- c("bind", "join", "operation", "convert", "final")

# Every registered name must be dispatchable. The counterpart of
# check_implemented() in tests/python/test_fixtures.py.
check_implemented <- function(d = load_design()) {
  errs <- character()
  tables <- list(scalar = SCALAR_OPS, window = WINDOW_OPS, aggregate = AGGREGATE_OPS)
  dispatch <- c(names(SCALAR_OPS), names(WINDOW_OPS), names(AGGREGATE_OPS))
  for (nm in names(d$ops)) {
    if (!(nm %in% dispatch)) {
      errs <- c(errs, sprintf("operation '%s' is registered but not implemented", nm))
      next
    }
    if (!(nm %in% names(tables[[d$ops[[nm]]$kind]]))) {
      errs <- c(errs, sprintf("operation '%s' declares kind '%s' but is implemented elsewhere",
                              nm, d$ops[[nm]]$kind))
    }
  }
  for (nm in names(d$exc)) {
    if (!(d$exc[[nm]]$stage %in% HANDLED_STAGES)) {
      errs <- c(errs, sprintf("exception '%s' binds to stage '%s', which the engine ignores",
                              nm, d$exc[[nm]]$stage))
    }
  }
  errs
}

read_csv_chr <- function(path) {
  df <- utils::read.csv(path, colClasses = "character", check.names = FALSE,
                        na.strings = character(0))
  lapply(seq_len(nrow(df)), function(i) as.list(df[i, , drop = FALSE]))
}

exception_map <- function(dv, exc_reg) {
  out <- list()
  for (ex in as_list_of(dv$exception)) {
    nm <- names(ex)[1]
    out[[exc_reg[[nm]]$stage]] <- list(name = nm, args = ex[[1]] %||% list())
  }
  out
}

run_spec <- function(path, d = load_design()) {
  spec <- read_yaml_1_2(path)
  base <- spec$base
  keys <- unlist(spec$keys %||% list())
  coltype <- stats::setNames(lapply(spec$columns, function(c) c$type),
                             vapply(spec$columns, function(c) c$name, ""))
  order_cols <- vapply(spec$columns, function(c) c$name, "")
  dir <- dirname(path)
  data <- lapply(spec$datasets, function(rel) read_csv_chr(file.path(dir, rel)))

  rows <- list()   # each: list(src=, driver=, tmpl=, out=list())

  templates <- spec$rows %||% list()
  if (length(templates)) {
    for (ti in seq_along(templates)) {
      t <- templates[[ti]]
      driver <- t$dataset %||% base
      for (rec in data[[driver]]) {
        if (!is.null(t$filter)) {
          look <- rec_lookup(rec, driver)
          if (!evaluate_predicate(t$filter, look)) next
        }
        rows[[length(rows) + 1L]] <- list(src = rec, driver = driver, tmpl = ti, out = list())
      }
    }
  } else {
    if (is.null(base)) stop("no rows entry and no base (R001)")
    for (rec in data[[base]]) {
      rows[[length(rows) + 1L]] <- list(src = rec, driver = base, tmpl = NA, out = list())
    }
  }

  ctx <- new.env(parent = emptyenv())
  ctx$data <- data
  ctx$rows <- rows
  ctx$keys <- keys
  ctx$base <- base
  ctx$coltype <- coltype
  ctx$spec <- spec
  ctx$ops <- d$ops
  ctx$exc <- d$exc

  # phase 1: row-template derivations
  for (ti in seq_along(templates)) {
    t <- templates[[ti]]
    idx <- which(vapply(ctx$rows, function(r) identical(r$tmpl, ti), logical(1)))
    driver <- t$dataset %||% base
    for (nm in toposort_defs(t$derivations, order_cols, ctx, driver)) {
      vals <- evaluate_derivation(nm, t$derivations[[nm]], idx, ctx, driver)
      for (k in seq_along(idx)) ctx$rows[[idx[k]]]$out[nm] <- list(vals[[k]])
    }
  }

  # phase 2: column derivations
  coldefs <- list()
  for (c in spec$columns) if (!is.null(c$derivation)) coldefs[[c$name]] <- c$derivation
  all_idx <- seq_along(ctx$rows)
  for (nm in toposort_defs(coldefs, order_cols, ctx, base)) {
    vals <- evaluate_derivation(nm, coldefs[[nm]], all_idx, ctx, base)
    for (k in all_idx) ctx$rows[[k]]$out[nm] <- list(vals[[k]])
  }

  # R005: order output rows by keys
  if (length(keys)) {
    kv <- lapply(keys, function(k) lapply(ctx$rows, function(r) r$out[[k]]))
    args <- unlist(lapply(kv, function(vals) {
      list(vapply(vals, function(v) ord_key(v)[1], 0),
           vapply(vals, function(v) ord_key(v)[2], 0),
           vapply(vals, function(v) if (is_missing(v)) "" else as.character(v), ""))
    }), recursive = FALSE)
    ctx$rows <- ctx$rows[do.call(order, args)]
  }

  lapply(ctx$rows, function(r) {
    stats::setNames(lapply(order_cols, function(c) serialize_value(r$out[[c]])), order_cols)
  })
}

rec_lookup <- function(rec, ds) {
  function(name) {
    key <- if (startsWith(name, paste0(ds, "."))) substring(name, nchar(ds) + 2L) else name
    rec[[key]]
  }
}

odm_item <- function(ctx, ds, item, src) {
  ctxkeys <- intersect(ODM_CONTEXT, names(src))
  hits <- Filter(function(r) {
    identical(r[["ItemOID"]], item) &&
      all(vapply(ctxkeys, function(k) identical(r[[k]], src[[k]]), logical(1)))
  }, ctx$data[[ds]])
  if (length(hits) > 1L) stop(sprintf("%s.%s: %d records match the context", ds, item, length(hits)))
  if (!length(hits)) return(NULL)
  hits[[1]][["Value"]]
}

joined_value <- function(ctx, ds, col, ri, dv) {
  right <- ctx$data[[ds]]
  if (!is.null(dv$filter)) {
    right <- Filter(function(r) evaluate_predicate(dv$filter, rec_lookup(r, ds)), right)
  }
  applicable <- if (length(right)) intersect(ctx$keys, names(right[[1]])) else character()
  if (!length(applicable)) stop(sprintf("%s.%s: no applicable keys (R003)", ds, col))

  ops <- as_list_of(dv$operations)
  reducing <- length(ops) && identical(ctx$ops[[names(ops[[1]])[1]]]$kind, "aggregate")
  want <- lapply(applicable, function(k) ctx$rows[[ri]]$out[[k]])
  hits <- Filter(function(r) {
    all(vapply(seq_along(applicable), function(j) {
      identical(as.character(r[[applicable[j]]]), as.character(want[[j]] %||% ""))
    }, logical(1)))
  }, right)

  if (reducing) {
    fn <- AGGREGATE_OPS[[names(ops[[1]])[1]]]
    return(fn(lapply(hits, function(h) h[[col]])))
  }
  if (length(hits) > 1L) {
    ex <- exception_map(dv, ctx$exc)$join
    if (is.null(ex) || !identical(ex$name, "multiple_matches")) {
      stop(sprintf("%s.%s: %d right-side matches (R003)", ds, col, length(hits)))
    }
    ocol <- sub("^.*\\.", "", if (is.list(ex$args$order_by)) ex$args$order_by$source else ex$args$order_by)
    idx <- sort_index(lapply(hits, function(h) h[[ocol]]))
    hits <- list(hits[[if (identical(ex$args$keep, "last")) idx[length(idx)] else idx[1]]])
  }
  if (!length(hits)) return(NULL)
  hits[[1]][[col]]
}

resolve_ref <- function(ctx, ref, ri, dv) {
  if (!grepl(".", ref, fixed = TRUE)) {
    r <- ctx$rows[[ri]]
    if (!(ref %in% names(r$out))) stop(sprintf("%s is not available yet (R001)", ref))
    return(r$out[[ref]])
  }
  parts <- strsplit(ref, ".", fixed = TRUE)[[1]]
  ds <- parts[1]; rest <- paste(parts[-1], collapse = ".")
  r <- ctx$rows[[ri]]
  if (identical(ds, r$driver)) {
    if (rest %in% names(r$src)) return(r$src[[rest]])
    return(odm_item(ctx, ds, rest, r$src))
  }
  first <- if (length(ctx$data[[ds]])) names(ctx$data[[ds]][[1]]) else character()
  if (rest %in% first) return(joined_value(ctx, ds, rest, ri, dv))
  odm_item(ctx, ds, rest, r$src)
}

resolve_args <- function(ctx, raw, ri, dv) {
  if (is.list(raw) && identical(names(raw), "source")) return(resolve_ref(ctx, raw$source, ri, dv))
  if (is.list(raw)) return(lapply(raw, function(v) resolve_args(ctx, v, ri, dv)))
  raw
}

deps_of <- function(dv, ctx, driver) {
  out <- character()
  src <- dv$source
  if (!is.null(src) && !grepl(".", src, fixed = TRUE)) out <- c(out, src)
  out <- c(out, unlist(dv$group_by %||% list()))
  if (!is.null(dv$where)) {
    v <- predicate_vars(dv$where)
    out <- c(out, v[!grepl(".", v, fixed = TRUE)])
  }
  if (!is.null(src) && grepl(".", src, fixed = TRUE) &&
      !identical(sub("\\..*$", "", src), driver %||% "")) {
    out <- c(out, ctx$keys)
  }
  collect <- function(x) {
    if (is.list(x)) {
      if (identical(names(x), "source") && !grepl(".", x$source, fixed = TRUE)) {
        out <<- c(out, x$source)
      } else {
        for (nm in names(x)) {
          if (identical(nm, "when") && is.character(x[[nm]])) {
            v <- predicate_vars(x[[nm]]); out <<- c(out, v[!grepl(".", v, fixed = TRUE)])
          } else collect(x[[nm]])
        }
        if (is.null(names(x))) for (e in x) collect(e)
      }
    }
  }
  for (op in as_list_of(dv$operations)) collect(op[[1]])
  for (ex in as_list_of(dv$exception)) collect(ex[[1]])
  unique(out)
}

toposort_defs <- function(defs, order_cols, ctx, driver) {
  pending <- names(defs); done <- character()
  pos <- stats::setNames(seq_along(order_cols), order_cols)
  while (length(pending)) {
    ready <- pending[vapply(pending, function(n) {
      d <- setdiff(intersect(deps_of(defs[[n]], ctx, driver), pending), n)
      !length(d)
    }, logical(1))]
    if (!length(ready)) stop(sprintf("dependency cycle among %s (R001)",
                                     paste(sort(pending), collapse = ", ")))
    ready <- ready[order(pos[ready])]
    done <- c(done, ready); pending <- setdiff(pending, ready)
  }
  done
}

evaluate_derivation <- function(name, dv, idx, ctx, driver) {
  exc <- exception_map(dv, ctx$exc)

  if (!is.null(dv$where)) {
    keep <- idx[vapply(idx, function(ri) {
      evaluate_predicate(dv$where, function(n) ctx$rows[[ri]]$out[[n]])
    }, logical(1))]
    sub_dv <- dv; sub_dv$where <- NULL
    sub <- evaluate_derivation(name, sub_dv, keep, ctx, driver)
    out <- vector("list", length(idx))
    for (k in seq_along(keep)) out[which(idx == keep[k])] <- list(sub[[k]])
    return(out)
  }

  vals <- lapply(idx, function(ri) {
    v <- tryCatch({
      if (!is.null(dv$literal)) dv$literal
      else if (!is.null(dv$source)) resolve_ref(ctx, dv$source, ri, dv)
      else NULL
    }, error = function(e) if (!is.null(exc$bind)) exc$bind$args$default else stop(e))
    if (is_missing(v) && !is.null(exc$bind)) v <- exc$bind$args$default
    v
  })

  for (op in as_list_of(dv$operations)) {
    opname <- names(op)[1]; raw <- op[[1]] %||% list()
    kind <- ctx$ops[[opname]]$kind
    if (identical(kind, "aggregate")) next            # applied during R003 reduction
    if (identical(kind, "window")) {
      vals <- apply_window(opname, raw, idx, vals, ctx, dv)
      next
    }
    fn <- SCALAR_OPS[[opname]]
    if (is.null(fn)) stop(sprintf("%s is registered but not implemented", opname))
    vals <- lapply(seq_along(idx), function(k) {
      ri <- idx[k]
      args <- resolve_args(ctx, raw, ri, dv)
      if (identical(opname, "call")) {
        args <- list(function_name = raw[["function"]],
                     args = resolve_args(ctx, raw$args %||% list(), ri, dv))
      }
      octx <- list(data = ctx$data, row_lookup = function(n) ctx$rows[[ri]]$out[[n]])
      tryCatch(fn(vals[[k]], args, octx), error = function(e) {
        if (identical(conditionMessage(e), UNMAPPED)) {
          if (is.null(exc$operation)) {
            stop(sprintf("%s: %s could not map a value and no unmapped exception is declared (R008)",
                         name, opname))
          }
          return(exc$operation$args$default)
        }
        stop(e)
      })
    })
  }

  conv <- lapply(vals, function(v) {
    tryCatch(convert_value(v, ctx$coltype[[name]]), error = function(e) {
      if (is.null(exc$convert)) stop(e)
      convert_value(exc$convert$args$default, ctx$coltype[[name]])
    })
  })

  if (!is.null(exc$final)) {
    conv <- lapply(seq_along(idx), function(k) {
      ri <- idx[k]; cur <- conv[[k]]
      for (rule in exc$final$args$rules) {
        look <- function(n) if (identical(n, name)) cur else ctx$rows[[ri]]$out[[n]]
        if (evaluate_predicate(rule$when, look)) return(convert_value(rule$value, ctx$coltype[[name]]))
      }
      cur
    })
  }
  conv
}

apply_window <- function(opname, raw, idx, vals, ctx, dv) {
  gb <- unlist(dv$group_by %||% list())
  gkey <- vapply(idx, function(ri) {
    paste(vapply(gb, function(k) as.character(ctx$rows[[ri]]$out[[k]] %||% ""), ""), collapse = "\r")
  }, "")
  out <- vector("list", length(idx))
  for (g in unique(gkey)) {
    sel <- which(gkey == g)
    sub_idx <- idx[sel]
    args <- list()
    for (nm in names(raw)) {
      spec <- raw[[nm]]
      if (is.list(spec) && is.null(names(spec))) {
        args[[paste0(nm, "_per_row")]] <- lapply(sub_idx, function(ri) {
          lapply(spec, function(s) resolve_args(ctx, s, ri, dv))
        })
      } else {
        args[[paste0(nm, "_per_row")]] <- lapply(sub_idx, function(ri) resolve_args(ctx, spec, ri, dv))
      }
    }
    res <- WINDOW_OPS[[opname]](as.list(sub_idx), args,
                               list(data = ctx$data, row_lookup = function(n) NULL))
    for (k in seq_along(sel)) out[sel[k]] <- list(res[[k]])
  }
  out
}
