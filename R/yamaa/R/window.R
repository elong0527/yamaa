# window.R -- window expressions (rules/operations/windows.md).
#
# eval_window_expr(kind, payload, ctx) -> tv of length ctx$n.
# The window reads completed output columns (column phase) or the row
# template's constructed rows (row phase, REQ-0326); a window that depends
# on another window's result fails during row construction.

eval_window_expr <- function(kind, payload, ctx) {
  w <- payload$window
  # REQ-1251: window accepts an inline mapping or a named reference string
  if (is.character(w) && length(w) == 1) {
    nw <- ctx$named_windows[[w]]
    if (is.null(nw))
      yamaa_error("unknown_window", paste0("unknown window: ", w))
    w <- nw
  }
  if (is.null(w)) yamaa_error("invalid_expression", paste0(kind, " needs a window"))
  # REQ-0340: position/order-moving windows need window.order_by
  if (kind %in% c("row_number", "rank", "row_value", "previous_non_missing", "locf") &&
      (is.null(w$order_by) || length(w$order_by) == 0))
    yamaa_error("window_order_by_required", paste0(kind, " requires window.order_by"))
  parts <- window_partitions(ctx, w)
  n <- ctx$n
  switch(kind,
    row_number = {
      out <- tv_na("int", n)
      for (p in parts) out$v[p$order] <- seq_along(p$order)
      out
    },
    rank = {
      method <- if (is.null(payload$method)) "competition" else payload$method
      # REQ-0287: method must be a string, not a mapping or other type
      if (!is.character(method) || length(method) != 1)
        yamaa_error("invalid_field_type", "rank method must be a string")
      out <- tv_na("int", n)
      for (p in parts) out$v[p$order] <- rank_from_ties(p$tie, method)
      out
    },
    row_value = {
      # REQ-0328: a row_value whose offset is zero fails; the current row's
      # own value is `source`, and a window must not be a second spelling
      offset <- payload$offset
      if (!is.null(offset) && as.integer(offset) == 0L)
        yamaa_error("zero_offset", "row_value offset must not be zero")
      src <- resolve_name(payload$source, ctx)
      offset <- as.integer(payload$offset)
      out <- tv_na(src$t, n)
      for (p in parts) {
        o <- p$order
        for (j in seq_along(o)) {
          k <- j + offset
          if (k >= 1 && k <= length(o)) out$v[o[j]] <- src$v[o[k]]
        }
      }
      out
    },
    previous_non_missing = ,
    locf = {
      if (!is.character(payload$source))
        yamaa_error("invalid_field_type",
          paste0(kind, " source must be a field reference"))
      carry_scan(resolve_name(payload$source, ctx), parts,
        use_current = kind == "locf")
    },
    baseline_flag = {
      d <- resolve_name(payload$date, ctx)
      r <- resolve_name(payload$reference_date, ctx)
      if (d$t != "date" || r$t != "date")
        yamaa_error("incompatible_input_type", "baseline_flag needs dates")
      out <- tv(rep(NA_character_, n), "str")
      for (p in parts) {
        o <- p$order
        dv <- d$v[o]; rv <- r$v[o]
        elig <- !is.na(dv) & !is.na(rv) & dv <= rv
        if (!any(elig)) next
        best <- max(dv[elig])  # canonical text: max = latest
        winners <- o[elig][dv[elig] == best]
        if (length(winners) > 1)
          yamaa_error("ambiguous_baseline", "baseline_flag: tied latest dates")
        out$v[winners] <- "Y"  # REQ-1127: missing elsewhere
      }
      out
    },
    yamaa_error("invalid_argument", paste0("unknown window kind: ", kind)))
}

# forward scan over ordered window partitions: every row takes its carried
# present source is written directly (locf, REQ-1239); without it the row
# takes the strictly earlier carried value (previous_non_missing, REQ-1126).
carry_scan <- function(src, parts, use_current) {
  out <- tv_na(src$t, length(src$v))
  for (p in parts) {
    o <- p$order; last <- tv_na(src$t, 1)$v
    for (j in seq_along(o)) {
      v <- src$v[o[j]]
      if (is.na(v)) {
        out$v[o[j]] <- last
      } else {
        out$v[o[j]] <- if (use_current) v else last
        last <- v
      }
    }
  }
  out
}

# partition rows: filter (TRUE only), group, order. Returns list of
window_partitions <- function(ctx, w) {
  n <- ctx$n
  elig <- rep(TRUE, n)
  if (!is.null(w$filter)) {
    # REQ-0287: filter must be a predicate (string), not another type
    if (!is.character(w$filter) || length(w$filter) != 1)
      yamaa_error("invalid_field_type", "window filter must be a predicate")
    resolver <- list(resolve = function(name) resolve_name(name, ctx), n = n)
    f <- eval_pred(parse_predicate_text(w$filter), resolver)
    elig <- !is.na(f) & f
  }
  rows <- which(elig)
  if (length(rows) == 0) return(list())
  gb <- w$group_by
  if (is.null(gb) || length(gb) == 0) {
    groups <- list(rows)
  } else {
    gkey <- vapply(rows, function(i)
      paste(vapply(gb, function(g) {
        c <- resolve_name(g, ctx); canon_key_text(c$t, c$v[i])
      }, character(1)), collapse = "\x1f"), character(1))
    # preserve first-appearance order of groups
    groups <- split(rows, factor(gkey, levels = unique(gkey)))
  }
  lapply(groups, function(gr) {
    oo <- order_rows_ties(ctx, w$order_by, gr)
    list(rows = gr, order = oo$order, tie = oo$tie)
  })
}

# competition/dense rank from tie-group ids (per ordered position)
rank_from_ties <- function(tie, method) {
  n <- length(tie)
  if (n == 0) return(integer(0))
  out <- integer(n)
  if (method == "competition") {
    for (i in seq_len(n)) out[i] <- which(tie == tie[i])[1]
  } else if (method == "dense") {
    ids <- match(tie, unique(tie))
    out <- ids
  } else yamaa_error("invalid_argument", paste0("rank method: ", method))
  out
}

# order a row subset by order_by terms; returns list(order, tie) where tie
# holds the tie-group id per ordered position. Stable: ties keep input order.
order_rows_ties <- function(ctx, terms, rows) {
  if (length(rows) == 0) return(list(order = integer(0), tie = integer(0)))
  keys <- order_keys(ctx, terms, rows)
  cmp <- make_row_cmp(keys)
  idx <- merge_sort_idx(seq_along(rows), cmp)
  tie <- integer(length(rows))
  if (length(rows) > 0) {
    g <- 1L; tie[idx[1]] <- 1L
    for (k in 2:length(rows)) {
      if (cmp(idx[k - 1], idx[k]) != 0) g <- g + 1L
      tie[idx[k]] <- g
    }
  }
  list(order = rows[idx], tie = tie[idx])
}

# backward-compatible wrapper returning only the order
order_rows <- function(ctx, terms, rows) {
  order_rows_ties(ctx, terms, rows)$order
}

order_keys <- function(ctx, terms, rows) {
  if (is.null(terms) || length(terms) == 0) return(list())
  lapply(terms, function(tm) {
    var <- if (is.character(tm)) tm else tm$variable
    dir <- if (is.character(tm) || is.null(tm$direction)) "asc" else tm$direction
    nulls <- if (is.character(tm) || is.null(tm$nulls)) "last" else tm$nulls
    c <- resolve_name(var, ctx)
    list(v = c$v[rows], t = c$t, dir = dir, nulls = nulls)
  })
}

make_row_cmp <- function(keys) {
  function(a, b) {
    # guard against empty indices (should not happen, but be safe)
    if (length(a) == 0 || length(b) == 0) return(0L)
    for (k in keys) {
      va <- k$v[a]; vb <- k$v[b]
      # guard against out-of-bounds (length 0)
      if (length(va) == 0 || length(vb) == 0) next
      na_a <- is.na(va); na_b <- is.na(vb)
      if (na_a && na_b) next
      if (na_a || na_b) {
        r <- if (k$nulls == "first") -1L else 1L
        return(if (na_a) r else -r)
      }
      c <- cmp_typed(tv(va, k$t), tv(vb, k$t))
      if (c != 0) {
        r <- as.integer(sign(c))
        return(if (k$dir == "desc") -r else r)
      }
    }
    0L
  }
}

merge_sort_idx <- function(idx, cmp) {
  n <- length(idx)
  if (n <= 1) return(idx)
  mid <- n %/% 2
  a <- merge_sort_idx(idx[1:mid], cmp)
  b <- merge_sort_idx(idx[(mid + 1):n], cmp)
  out <- integer(n); i <- 1L; j <- 1L; k <- 1L
  while (i <= length(a) && j <= length(b)) {
    if (cmp(a[i], b[j]) <= 0) { out[k] <- a[i]; i <- i + 1L }
    else { out[k] <- b[j]; j <- j + 1L }
    k <- k + 1L
  }
  while (i <= length(a)) { out[k] <- a[i]; i <- i + 1L; k <- k + 1L }
  while (j <= length(b)) { out[k] <- b[j]; j <- j + 1L; k <- k + 1L }
  out
}

# eval_order_terms(terms, resolver) -> integer vector ranking each element
# (for multiple_matches / lookup order_by over record resolvers)
eval_order_terms <- function(terms, resolver) {
  n <- resolver$n
  if (is.null(terms) || length(terms) == 0) return(rep(1L, n))
  # compute a total order key per element via successive ranking
  ord <- order_records(resolver, terms, seq_len(n))
  rank <- integer(n)
  rank[ord] <- seq_len(n)
  rank
}

# order record positions by terms using the resolver; stable.
order_records <- function(resolver, terms, pos) {
  keys <- lapply(terms, function(tm) {
    var <- if (is.character(tm)) tm else tm$variable
    dir <- if (is.character(tm) || is.null(tm$direction)) "asc" else tm$direction
    nulls <- if (is.character(tm) || is.null(tm$nulls)) "last" else tm$nulls
    c <- resolver$resolve(var)
    list(v = c$v[pos], t = c$t, dir = dir, nulls = nulls)
  })
  cmp <- function(a, b) {
    for (k in keys) {
      va <- k$v[a]; vb <- k$v[b]
      na_a <- is.na(va); na_b <- is.na(vb)
      if (na_a && na_b) next
      if (na_a || na_b) {
        r <- if (k$nulls == "first") -1L else 1L
        return(if (na_a) r else -r)
      }
      c <- cmp_typed(tv(va, k$t), tv(vb, k$t))
      if (c != 0) {
        r <- as.integer(sign(c))
        return(if (k$dir == "desc") -r else r)
      }
    }
    0L
  }
  idx <- merge_sort_idx(seq_along(pos), cmp)
  pos[idx]
}
