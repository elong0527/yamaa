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
    # REQ-1263: derivations may read other named intermediates. The donor
    # ctx carries the real intermediate specs, the full inputs (the
    # synthetic per-donor-row ctx swaps in a 1-row augmented donor frame),
    # the current augmented donor frame, a per-pass read cache, and the
    # read stack seeded with this intermediate (self-reads are cycles).
    inter_specs = ctx$inter_specs,
    all_inputs = ctx$inputs,
    donor_ydf = ydf,
    donor_read_cache = new.env(hash = TRUE, parent = emptyenv()),
    inter_read_stack = c(ctx$inter_read_stack, spec$id),
    inter = new.env(parent = emptyenv()),
    keys = character(0),
    phase = "derivation",
    # REQ-1185: the dataset qualifier inside an intermediate's derivations
    # names the derivation-augmented donor frame (stored + earlier-derived
    # names in col); resolve_name short-circuits on this marker.
    inter_ds = ds,
    inter_donor = list(id = spec$id, ds = ds),
    donor_ydf_rows = seq_len(nrow(ydf)),
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
    attr(ydf, "coltypes") <- cts
    # later derivations (and their windows) read earlier-derived names
    # bare through the context, alongside the stored fields
    dctx$col[[nm]] <- val
    dctx$donor_ydf <- ydf
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
      # REQ-1248: an intermediate declaring nothing beyond id, dataset, and
      # no_match merely renames the qualifier and fails
      if (length(setdiff(names(im), c("id", "dataset", "no_match"))) == 0)
        yamaa_error("rename_only_intermediate",
          paste0("intermediate ", im$id, " only renames dataset ", im$dataset))
      # REQ-0153: an omitted key is the output keys the dataset also
      # carries; with no applicable key the run is rejected here, before
      # any data is read (not lazily on first read). Note: use exact name
      # matching -- `$key` partial-matches.
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

# per-donor-record synthetic context for REQ-1263 (see DESIGN.md).
donor_row_ctx <- function(ctx, j) {
  ydf <- ctx$donor_ydf
  if (!is.null(ctx$donor_ydf_rows)) j <- ctx$donor_ydf_rows[j]
  cts <- attr(ydf, "coltypes")
  # the record under evaluation reads through col (REQ-1185: a derivation
  # context's dataset qualifier names the derivation-augmented donor
  # frame); other records fail unjoinable.
  dcol <- lapply(names(ydf), function(nm) tv(ydf[[nm]][j], cts[[nm]]))
  names(dcol) <- names(ydf)
  c(
    ctx[names(ctx) %in% setdiff(names(ctx), c("n", "col", "inputs", "driver_rec", "driver_ds", "inter", "inter_cache"))],
    list(
      n = 1L,
      col = dcol,
      # the target intermediate's selection needs the full input frames;
      # the donor frame itself stays available as ctx$donor_ydf
      inputs = ctx$all_inputs,
      inter_donor = ctx$inter_donor,
      inter_read_stack = ctx$inter_read_stack,
      donor_ydf = ctx$donor_ydf,
      donor_read_cache = ctx$donor_read_cache,
      # the synthetic per-record ctx reads through col (dcol); nothing
      # downstream may see a donor subset
      .rows = NULL, .full_n = NULL, block_inter_read = NULL
    )
  )
}

# REQ-1263: an intermediate read inside another intermediate's derivations
# runs per donor record. Resolves the match key against the donor record,
# selects from the target's (possibly derived) frame, and answers the
# field -- caching per (target id, donor record) for the pass.
inter_read_donor <- function(ctx, id, v) {
  if (id %in% ctx$inter_read_stack)
    yamaa_error("dependency_cycle", paste0("intermediate dependency cycle at: ", id))
  if (identical(id, ctx$inter_donor$id))
    yamaa_error("dependency_cycle", paste0("intermediate ", id, " reads itself"))
  if (is.null(ctx$donor_ydf_rows)) yamaa_error("invalid_spec", "donor rows unset")
  n <- length(ctx$donor_ydf_rows)
  # no donor records: the read's type is undeterminable -- fail loudly
  # rather than assembling a wrongly-typed empty
  if (n == 0)
    yamaa_error("invalid_spec",
      paste0("intermediate ", ctx$inter_donor$id, " reads ", id, " with no donor records"))
  out <- vector("list", n)
  for (k in seq_len(n)) {
    ck <- paste0(id, "\x1f", ctx$donor_ydf_rows[k])
    hit <- get0(ck, envir = ctx$donor_read_cache, ifnotfound = NULL)
    if (!is.null(hit)) {
      out[[k]] <- hit
      next
    }
    sctx <- donor_row_ctx(ctx, k)
    sctx$inter_cache <- new.env(parent = emptyenv())
    sctx$inter_read_stack <- c(sctx$inter_read_stack, id)
    ensure_intermediate(sctx, id)
    entry <- get(id, envir = sctx$inter_cache, inherits = FALSE)
    spec <- sctx$inter_specs[[id]]
    if (is.null(spec)) yamaa_error("unknown_field", paste0("unknown intermediate: ", id))
    ydf <- if (!is.null(entry$ydf)) entry$ydf else sctx$all_inputs[[spec$dataset]]
    if (!v %in% names(ydf))
      yamaa_error("unknown_field", paste0(id, ".", v, ": no such field"))
    t <- attr(ydf, "coltypes")[[v]]
    sel <- entry$sel
    if (length(sel) != 1) yamaa_error("invalid_spec", "donor read selection not scalar")
    # REQ-0124: an empty selection with no `no_match` fails `unmatched_key`
    # per row (the rows here are all read); a `no_match` answers instead.
    has_nm <- "no_match" %in% names(spec)
    if (is.na(sel)) {
      if (!has_nm) yamaa_error("unmatched_key", paste0("intermediate ", id, " has no record"))
      val <- if (is.null(spec$no_match)) tv_na(t, 1) else literal_to_tv(spec$no_match, t, 1)
    } else {
      val <- tv(ydf[[v]][sel], t)
    }
    assign(ck, val, envir = ctx$donor_read_cache)
    out[[k]] <- val
  }
  # reassemble: every piece is a length-1 tv of a common type
  vals <- unlist(lapply(out, function(x) x$v), use.names = FALSE)
  tv(vals, out[[1]]$t)
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
    sel$for_rows <- ctx$.rows
    assign(id, sel, envir = ctx$inter_cache)
    return(invisible(NULL))
  }
  if (exists(id, envir = ctx$inter_cache, inherits = FALSE)) return(invisible(NULL))
  sel <- compute_intermediate_sel(ctx, spec)
  # the row set this selection covers: a masked case-branch retry computes
  # for its taken rows only (ctx$.rows); inter_read maps or recomputes.
  sel$for_rows <- ctx$.rows
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

  # unified match key (REQ-0115): (dataset-column, match-entry) pairs.
  # Exact name match: `$key` partial-matches.
  pairs <- parse_match_key(if ("key" %in% names(spec)) spec[["key"]] else NULL)
  if (is.null(pairs)) {
    # REQ-0153: an omitted key pairs the output keys the dataset carries,
    # each with the same-named current-row value.
    dkey <- intersect(ctx$keys, names(ydf))
    if (length(dkey) == 0)
      yamaa_error("no_applicable_keys",
        paste0("intermediate ", spec$id, " has no applicable keys"))
    pairs <- lapply(dkey, function(k) list(right = k, entry = k))
  }
  if (length(pairs) == 0)
    yamaa_error("invalid_field_type",
      paste0("intermediate ", spec$id, ": key must not be empty"))
  dkey <- vapply(pairs, function(p) p$right, character(1))
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
    rk <- paste(vapply(pairs, function(p) match_entry_text(ctx, p$entry, i),
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
