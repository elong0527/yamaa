# lookup.R -- named intermediates (rules/operations/lookup.md).
#
# An intermediate selects one record per output row from its dataset.
# Selection is lazy: ensure_intermediate(ctx, id) computes and caches the
# per-row selection the first time the intermediate is read. The cache lives
# in an environment (reference semantics) inside ctx.

init_intermediates <- function(spec, ctx) {
  specs <- list()
  if (!is.null(spec$intermediates)) {
    for (im in spec$intermediates) {
      # REQ-0152: intermediate id must not collide with input dataset ids
      if (im$id %in% names(ctx$inputs))
        yamaa_error("duplicate_identifier",
          paste0("intermediate id collides with input dataset: ", im$id))
      # REQ-0153: an omitted key is the output keys the dataset also
      # carries; with no applicable key the run is rejected here, before
      # any data is read (not lazily on first read). Note: use exact name
      # matching -- `$key` would partially match `key_base`.
      if (!"key" %in% names(im)) {
        ds <- im$dataset
        if (!is.null(ds) && ds %in% names(ctx$inputs)) {
          dkey <- intersect(ctx$keys, names(ctx$inputs[[ds]]))
          if (length(dkey) == 0)
            yamaa_error("no_applicable_keys",
              paste0("intermediate ", im$id, " has no applicable keys"))
        }
      }
      specs[[im$id]] <- im
    }
  }
  ctx$inter_specs <- specs
  ctx$inter_cache <- new.env(parent = emptyenv())
  # legacy name used by resolve_name; points at the cache env
  ctx$inter <- ctx$inter_cache
  ctx
}

ensure_intermediate <- function(ctx, id) {
  if (exists(id, envir = ctx$inter_cache, inherits = FALSE)) return(invisible(NULL))
  spec <- ctx$inter_specs[[id]]
  if (is.null(spec)) yamaa_error("unknown_field", paste0("unknown intermediate: ", id))
  sel <- compute_intermediate_sel(ctx, spec)
  assign(id, sel, envir = ctx$inter_cache)
  invisible(NULL)
}

# per-row selected record index (NA = absent) + absent flag
compute_intermediate_sel <- function(ctx, spec) {
  ds <- spec$dataset
  if (!ds %in% names(ctx$inputs)) yamaa_error("unknown_field", paste0("dataset: ", ds))
  ydf <- ctx$inputs[[ds]]
  n <- ctx$n
  # eligible records after the dataset's own filter
  recs <- seq_len(nrow(ydf))
  recs <- apply_record_filter(ctx, ds, recs, spec$filter)

  # key pairs: key (dataset cols) with key_base (row vars).
  # Exact name match: `$key` would partially match `key_base`.
  dkey <- if ("key" %in% names(spec)) spec[["key"]] else NULL
  bkey <- spec$key_base
  dkey_inferred <- is.null(dkey)
  if (dkey_inferred) {
    dkey <- intersect(ctx$keys, names(ydf))
    # REQ-0153: no applicable key fails
    if (length(dkey) == 0)
      yamaa_error("no_applicable_keys",
        paste0("intermediate ", spec$id, " has no applicable keys"))
    if (is.null(bkey)) bkey <- dkey
  } else {
    if (is.null(bkey)) bkey <- dkey
  }
  dkey <- as.character(dkey); bkey <- as.character(bkey)
  if (length(dkey) == 0 || length(dkey) != length(bkey))
    yamaa_error("source_key_length_mismatch", paste0("intermediate ", spec$id))
  for (k in dkey) {
    if (!k %in% names(ydf)) yamaa_error("unknown_field", paste0(spec$id, ": ", k))
  }

  # index eligible records by dataset key
  dcts <- attr(ydf, "coltypes")
  rkey <- vapply(recs, function(r)
    paste(vapply(seq_along(dkey), function(j)
      canon_key_text(dcts[[dkey[j]]], ydf[[dkey[j]]][r]), character(1)),
      collapse = "\x1f"), character(1))
  idx <- split(recs, factor(rkey, levels = unique(rkey)))

  # between bounds
  btw <- spec$between
  if (!is.null(btw)) {
    bv <- resolve_name(btw$value, ctx)  # row variable
    lo_c <- ydf[[btw$lower]]; hi_c <- ydf[[btw$upper]]
    lo_t <- dcts[[btw$lower]]; hi_t <- dcts[[btw$upper]]
    # REQ-0121: value and bound types must be comparable
    comparable <- function(a, b) {
      a == b || (a %in% c("int", "float") && b %in% c("int", "float"))
    }
    if (!comparable(bv$t, lo_t) || !comparable(bv$t, hi_t))
      yamaa_error("incomparable_range_types",
        paste0("intermediate ", spec$id, ": between value type ", bv$t,
          " not comparable with bound types ", lo_t, "/", hi_t))
  }

  has_order <- !is.null(spec$order_by)
  if (has_order && is.null(spec$keep))
    yamaa_error("unpaired_fields", paste0("intermediate ", spec$id, ": order_by without keep"))
  if (!has_order && !is.null(spec$keep))
    yamaa_error("unpaired_fields", paste0("intermediate ", spec$id, ": keep without order_by"))

  sel <- rep(NA_integer_, n)
  absent <- rep(FALSE, n)
  for (i in seq_len(n)) {
    rk <- paste(vapply(bkey, function(k) row_key_text(ctx, k, i), character(1)),
      collapse = "\x1f")
    m <- idx[[rk]]
    if (is.null(m)) m <- integer(0)
    if (!is.null(btw)) {
      rv <- bv$v[i]
      keep_m <- vapply(m, function(r) {
        lo <- lo_c[r]; hi <- hi_c[r]
        if (is.na(lo) || is.na(hi) || is.na(rv)) return(FALSE)
        cmp_typed(tv(lo, lo_t), tv(rv, bv$t)) <= 0 &&
          cmp_typed(tv(rv, bv$t), tv(hi, hi_t)) <= 0
      }, logical(1))
      m <- m[keep_m]
    }
    if (length(m) == 0) {
      absent[i] <- TRUE
      next
    }
    if (length(m) > 1) {
      if (!has_order)
        yamaa_error("multiple_matches",
          paste0("intermediate ", spec$id, ": ", length(m), " records for row ", i))
      resolver <- list(resolve = function(v) {
        s <- split_qual(v)
        tv(ydf[[s$v]][m], dcts[[s$v]])
      })
      m <- m[order_records(resolver, spec$order_by, seq_along(m))]
      m <- if (spec$keep == "first") m[1] else m[length(m)]
    }
    sel[i] <- m
  }
  list(sel = sel, absent = absent)
}
