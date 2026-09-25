# lookup.R -- named intermediates (rules/operations/lookup.md).
#
# An intermediate selects one record per output row from its dataset.
# Selection is lazy: ensure_intermediate(ctx, id) computes and caches the
# per-row selection the first time the intermediate is read. The cache lives
# in an environment (reference semantics) inside ctx.

# derive_intermediate_frame(ctx, spec, ds): REQ-1185. Each derivation is
# computed once per record of the intermediate's dataset, and the derived
# values augment the donor records before filter, matching, order_by
# selection, and column reads. The derivation context sees only the
# dataset's stored fields: a bare name reads a stored field and a qualified
# name must name the dataset; anything else (a driver field, another
# intermediate, a sibling derivation) fails as unknown_field. A derived
# name shadowing a stored column fails as duplicate_derivation.
derive_intermediate_frame <- function(ctx, spec, ds) {
  ydf <- ctx$inputs[[ds]]
  derivs <- spec$derivations
  if (is.null(derivs) || length(derivs) == 0) return(ydf)
  cts <- attr(ydf, "coltypes")
  stored <- names(ydf)
  dcol <- lapply(stored, function(v) tv(ydf[[v]], cts[[v]]))
  names(dcol) <- stored
  dctx <- list(
    n = nrow(ydf),
    col = dcol,
    inputs = setNames(list(ydf), ds),
    inter_specs = list(),
    inter = new.env(parent = emptyenv()),
    keys = character(0),
    phase = "derivation",
    spec = ctx$spec,
    spec_dir = ctx$spec_dir,
    project_fns = ctx$project_fns,
    project_env = ctx$project_env,
    warn_env = ctx$warn_env,
    check_env = ctx$check_env
  )
  for (nm in names(derivs)) {
    if (nm %in% names(ydf))
      yamaa_error("duplicate_derivation",
        paste0("intermediate ", spec$id, ": derived name shadows stored column: ", nm))
    val <- eval_expression(derivs[[nm]], dctx)
    if (tv_len(val) != nrow(ydf))
      yamaa_error("invalid_spec",
        paste0("intermediate ", spec$id, ": derivation ", nm, " returned ",
          tv_len(val), " values for ", nrow(ydf), " records"))
    ydf[[nm]] <- val$v
    cts[[nm]] <- val$t
  }
  attr(ydf, "coltypes") <- cts
  ydf
}

init_intermediates <- function(spec, ctx) {
  specs <- list()
  if (!is.null(spec$intermediates)) {
    for (im in spec$intermediates) {
      # REQ-0152: intermediate id must not collide with input dataset ids
      if (im$id %in% names(ctx$inputs))
        yamaa_error("duplicate_identifier",
          paste0("intermediate id collides with input dataset: ", im$id))
      # REQ-1248: an intermediate declaring only id and dataset merely
      # renames the qualifier and fails
      if (length(setdiff(names(im), c("id", "dataset"))) == 0)
        yamaa_error("rename_only_intermediate",
          paste0("intermediate ", im$id, " only renames dataset ", im$dataset))
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
  spec <- ctx$inter_specs[[id]]
  if (is.null(spec)) yamaa_error("unknown_field", paste0("unknown intermediate: ", id))
  if (identical(spec$dataset, "SELF")) {
    # REQ-0120: the donor set is the completed row records, which grows as
    # row construction proceeds -- a cached selection would go stale, so a
    # SELF selection is recomputed on every read. A read with no completed
    # rows (the first template during row construction) fails phase_boundary.
    donors <- ctx$inputs[["SELF"]]
    if (is.null(donors) || nrow(donors) == 0)
      yamaa_error("phase_boundary",
        paste0("SELF intermediate ", id, " read with no completed rows"))
    sel <- compute_intermediate_sel(ctx, spec)
    assign(id, sel, envir = ctx$inter_cache)
    return(invisible(NULL))
  }
  if (exists(id, envir = ctx$inter_cache, inherits = FALSE)) return(invisible(NULL))
  sel <- compute_intermediate_sel(ctx, spec)
  assign(id, sel, envir = ctx$inter_cache)
  invisible(NULL)
}

# per-row selected record index (NA = absent) + absent flag
compute_intermediate_sel <- function(ctx, spec) {
  ds <- spec$dataset
  if (!ds %in% names(ctx$inputs)) yamaa_error("unknown_field", paste0("dataset: ", ds))
  # REQ-1185: derived names augment the donor records before the filter,
  # matching, and order_by selection below.
  ctx$inputs[[ds]] <- derive_intermediate_frame(ctx, spec, ds)
  ydf <- ctx$inputs[[ds]]
  n <- ctx$n
  # eligible records after the dataset's own filter.
  # REQ-0132/0133: a donor-only filter applies once per run; a filter naming
  # the driver dataset is evaluated per row against equality-matched donor
  # records. The driver qualifier must be the driver of every row template.
  recs <- seq_len(nrow(ydf))
  sc <- filter_scope(ctx, ds, spec$filter, spec$id)
  fnode <- sc$fnode; corr_q <- sc$corr_q
  if (length(corr_q) == 0)
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
  dkey <- as.character(dkey); bkey <- normalize_key_base(bkey)
  if (length(dkey) == 0 || length(dkey) != length(bkey))
    yamaa_error("source_key_length_mismatch", paste0("intermediate ", spec$id))
  for (k in dkey) {
    if (!k %in% names(ydf)) yamaa_error("unknown_field", paste0(spec$id, ": ", k))
  }

  # index eligible records by dataset key
  dcts <- attr(ydf, "coltypes")
  ver <- spec$verification
  if (!is.null(ver) && !is.null(ver$unique)) {
    if (length(corr_q) > 0)
      yamaa_error("correlated_filter_with_unique_verification",
        paste0("intermediate ", spec$id,
          ": verification cannot combine with a correlated filter"))
    ucols <- as.character(ver$unique)
    for (uc in ucols)
      if (!uc %in% names(ydf))
        yamaa_error("unknown_field",
          paste0("intermediate ", spec$id, ": unique column ", uc, " not found"))
    ukey <- vapply(recs, function(r)
      paste(vapply(ucols, function(uc)
        canon_key_text(dcts[[uc]], ydf[[uc]][r]), character(1)),
        collapse = "\x1f"), character(1))
    if (anyDuplicated(ukey) > 0)
      yamaa_error("duplicate_intermediate_records",
        paste0("intermediate ", spec$id, ": duplicate unique combination"))
  }
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
    rk <- paste(vapply(bkey, function(e) key_base_entry_text(ctx, e, i),
      character(1)), collapse = "\x1f")
    m <- idx[[rk]]
    if (is.null(m)) m <- integer(0)
    # REQ-0133: the correlated filter applies per row against the matched
    # donors, before range narrowing and ordered selection.
    if (length(corr_q) > 0 && length(m) > 0) {
      r <- make_correlated_resolver(ctx, ds, m, corr_q, ctx$driver_rec[[i]])
      keep <- eval_pred(fnode, r)
      m <- m[!is.na(keep) & keep]
    }
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
  list(sel = sel, absent = absent, ydf = ydf)
}
