# expr.R -- expression evaluation (rules/operations/expressions.md).
#
# eval_expression(expr, ctx) evaluates the single-key expression map and
# returns a tv of length ctx$n. ctx is documented in engine.R; the fields
# used here:
#   n, col (named tv of completed columns), coltypes, inputs (named ydf),
#   keys, base, driver_ds (char per row), driver_rec (list of int vec per row),
#   inter (id -> list(sel, absent)), inter_specs, spec, phase.

EXPR_KINDS <- c("source", "literal", "first_available", "greatest", "least",
  "case", "mapping", "cut", "compute", "aggregate", "lookup", "function",
  "row_number", "rank", "row_value", "previous_non_missing", "baseline_flag",
  "to_date", "date_diff", "date_impute", "date_precision", "study_day",
  "to_epoch_day",
  "datetime_impute", "datetime_precision",
  "str_upper", "str_lower", "str_extract", "str_concat", "str_template",
  "round_half_away_from_zero", "value")

eval_expression <- function(expr, ctx) {
  if (is.character(expr) && length(expr) == 1) {
    # concise form: a variable (REQ-0083 / REQ-1057)
    return(eval_expression(list(source = expr), ctx))
  }
  if (!is.list(expr) || length(expr) != 1 || is.null(names(expr)))
    yamaa_error("invalid_expression", "an expression names exactly one class")
  kind <- names(expr)[1]; payload <- expr[[1]]
  if (!kind %in% EXPR_KINDS)
    yamaa_error("invalid_expression", paste0("unknown expression class: ", kind))
  switch(kind,
    source = eval_source_expr(payload, ctx),
    literal = eval_literal(payload, ctx),
    first_available = eval_first_available(payload, ctx),
    greatest = , least = eval_greatest_least(kind, payload, ctx),
    case = eval_case(payload, ctx),
    mapping = eval_mapping(payload, ctx),
    cut = eval_cut(payload, ctx),
    compute = eval_compute_expr(payload, ctx),
    aggregate = eval_aggregate(payload, ctx),
    lookup = eval_lookup_inline(payload, ctx),
    row_number = , rank = , row_value = ,
    previous_non_missing = , baseline_flag = eval_window_expr(kind, payload, ctx),
    to_date = eval_to_date(payload, ctx),
    to_epoch_day = eval_to_epoch_day(payload, ctx),
    "function" = eval_function_expr(payload, ctx),
    date_diff = eval_date_diff(payload, ctx),
    date_impute = eval_date_impute(payload, ctx),
    date_precision = eval_date_precision(payload, ctx),
    datetime_impute = eval_datetime_impute(payload, ctx),
    datetime_precision = eval_datetime_precision(payload, ctx),
    study_day = eval_study_day(payload, ctx),
    str_upper = eval_str_case(kind, payload, ctx),
    str_lower = eval_str_case(kind, payload, ctx),
    str_extract = eval_str_extract(payload, ctx),
    str_concat = eval_str_concat(payload, ctx),
    str_template = eval_str_template(payload, ctx),
    round_half_away_from_zero = eval_round_half(payload, ctx),
    value = eval_expression(payload, ctx))
}

# ---- name resolution ------------------------------------------------------
split_qual <- function(name) {
  parts <- strsplit(name, ".", fixed = TRUE)[[1]]
  if (length(parts) == 1) list(q = NULL, v = name)
  else list(q = parts[1], v = paste(parts[-1], collapse = "."))
}

resolve_name <- function(name, ctx) {
  s <- split_qual(name)
  if (is.null(s$q)) {
    if (s$v %in% names(ctx$col)) return(ctx$col[[s$v]])
    yamaa_error("unknown_field", paste0("unresolved reference: ", name))
  }
  q <- s$q; v <- s$v
  if (q %in% names(ctx$inter_specs)) return(inter_read(ctx, q, v))
  if (q %in% names(ctx$inputs)) return(dataset_scalar_read(ctx, q, v, NULL))
  yamaa_error("unknown_field", paste0("unknown qualifier: ", q, " in ", name))
}

# read of V from an intermediate's selected record (per row)
inter_read <- function(ctx, id, v) {
  ensure_intermediate(ctx, id)
  spec <- ctx$inter_specs[[id]]
  ydf <- ctx$inputs[[spec$dataset]]
  if (!v %in% names(ydf)) yamaa_error("unknown_field", paste0(id, ".", v, ": no such field"))
  t <- attr(ydf, "coltypes")[[v]]
  entry <- get(id, envir = ctx$inter_cache, inherits = FALSE)
  sel <- entry$sel; absent <- entry$absent
  if (isTRUE(spec$strict) && any(absent))
    yamaa_error("unmatched_key", paste0("strict intermediate ", id, " has no record for some row"))
  vals <- ydf[[v]][sel]  # NA where sel is NA
  vals[absent] <- NA
  missing_lit <- spec$missing
  if (!is.null(missing_lit) && any(absent)) {
    lit <- literal_to_tv(missing_lit, t, sum(absent))
    vals[absent] <- lit$v
  }
  tv(vals, t)
}

# scalar qualified read of dataset q's field v for every row (REQ-0085/0086).
# binding may carry filter/multiple_matches/missing (source expression).
dataset_scalar_read <- function(ctx, q, v, binding) {
  ydf <- ctx$inputs[[q]]
  if (!v %in% names(ydf)) yamaa_error("unknown_field", paste0(q, ".", v, ": no such field"))
  t <- attr(ydf, "coltypes")[[v]]
  n <- ctx$n
  out <- tv_na(t, n)
  missing_lit <- if (!is.null(binding)) binding$missing else NULL
  for (i in seq_len(n)) {
    recs <- if (q == ctx$driver_ds[i]) ctx$driver_rec[[i]]
            else implicit_join_matches(ctx, q, i)
    recs <- apply_record_filter(ctx, q, recs, if (!is.null(binding)) binding$filter else NULL)
    recs <- apply_multiple_matches(ctx, q, recs, if (!is.null(binding)) binding$multiple_matches else NULL)
    out$v[i] <- scalar_count_read(ydf[[v]], recs, missing_lit, t,
      paste0(q, ".", v))
  }
  out
}

# REQ-0044: 0 -> missing policy; 1 -> the value; >1 -> one value or fail
# REQ-0111: absence (no matching records) yields typed missing.
# REQ-0075: more than one value for one key combination fails.
scalar_count_read <- function(col, recs, missing_lit, t, what) {
  if (length(recs) == 0) return(tv_na(t, 1)$v)
  vals <- col[recs]
  if (all(is.na(vals))) {
    if (!is.null(missing_lit)) return(literal_to_tv(missing_lit, t, 1)$v)
    return(vals[1])
  }
  if (length(recs) == 1) return(vals)
  u <- unique(vals[!is.na(vals)])
  if (length(u) == 1) return(u)
  yamaa_error("multiple_values_per_key", paste0("multiple values for ", what))
}

# the current row's value of key k as canonical text. Precedence: derived
# output column, row-construction key values, driver record's key field.
row_key_text <- function(ctx, k, i) {
  if (k %in% names(ctx$col)) {
    c <- ctx$col[[k]]; return(canon_key_text(c$t, c$v[i]))
  }
  rk <- ctx$row_keys[[k]]
  if (!is.null(rk)) return(canon_key_text(rk$t, rk$v[i]))
  s <- split_qual(k)
  dds <- ctx$driver_ds[i]
  # a qualified key_base term resolves against the driver record when it
  # names the driver dataset; any other qualifier has no current-row value
  if (!is.null(s$q) && (is.null(dds) || s$q != dds))
    yamaa_error("unknown_field", paste0("no key value for ", k, " in row ", i))
  v <- if (is.null(s$q)) k else s$v
  if (!is.null(dds)) {
    ddf <- ctx$inputs[[dds]]
    if (v %in% names(ddf)) {
      r <- ctx$driver_rec[[i]]
      ct <- attr(ddf, "coltypes")[[v]]
      vals <- ddf[[v]][r]
      if (length(r) == 1) return(canon_key_text(ct, vals))
      if (all(is.na(vals))) return(canon_key_text(ct, vals[1]))
      u <- unique(vals[!is.na(vals)])
      if (length(u) == 1) return(canon_key_text(ct, u))
      yamaa_error("multiple_values_per_key", paste0("driver key ", k, " not constant"))
    }
  }
  yamaa_error("unknown_field", paste0("no key value for ", k, " in row ", i))
}

# implicit join: records of q sharing the applicable keys with row i.
implicit_join_matches <- function(ctx, q, i) {
  ydf <- ctx$inputs[[q]]
  keys <- intersect(ctx$keys, names(ydf))
  if (length(keys) == 0)
    yamaa_error("unjoinable", paste0("no shared key between output and ", q))
  idx <- get_key_index(ctx, q, keys)
  keyvals <- vapply(keys, function(k) row_key_text(ctx, k, i), character(1))
  key <- paste(keyvals, collapse = "\x1f")
  m <- idx[[key]]
  if (is.null(m)) integer(0) else m
}

canon_key_text <- function(t, v) {
  if (length(v) == 0) return("\x1eNA\x1e")
  v <- v[1]
  if (is.na(v)) return("\x1eNA\x1e")
  switch(t, str = v, int = as.character(v), float = float_text(v),
    date = v, datetime = v)
}

# cached per-(dataset, keys) record index: key string -> int vector
get_key_index <- function(ctx, q, keys) {
  ck <- paste0(q, "\x1f", paste(keys, collapse = "\x1f"))
  if (!is.null(ctx$key_indexes[[ck]])) return(ctx$key_indexes[[ck]])
  ydf <- ctx$inputs[[q]]
  n <- nrow(ydf)
  env <- new.env(hash = TRUE, parent = emptyenv())
  if (n > 0) {
    keyvec <- vapply(seq_len(n), function(r) {
      paste(vapply(keys, function(k)
        canon_key_text(attr(ydf, "coltypes")[[k]], ydf[[k]][r]), character(1)),
        collapse = "\x1f")
    }, character(1))
    # split is faster than per-row assign
    sp <- split(seq_len(n), keyvec)
    for (kk in names(sp)) env[[kk]] <- sp[[kk]]
  }
  ctx$key_indexes[[ck]] <- env
  env
}

# predicate over a dataset's records: returns the TRUE record indices
apply_record_filter <- function(ctx, q, recs, filter) {
  if (is.null(filter) || length(recs) == 0) return(recs)
  ydf <- ctx$inputs[[q]]
  r <- make_record_resolver(ctx, q, recs)
  keep <- eval_pred(parse_predicate_text(filter), r)
  recs[!is.na(keep) & keep]
}

# record resolver: names resolve against one dataset's records --------------
make_record_resolver <- function(ctx, q, recs) {
  ydf <- ctx$inputs[[q]]
  cts <- attr(ydf, "coltypes")
  n <- length(recs)
  resolve <- function(name) {
    s <- split_qual(name)
    if (is.null(s$q)) {
      if (s$v %in% names(ydf)) return(tv(ydf[[s$v]][recs], cts[[s$v]]))
      yamaa_error("unknown_field", paste0("no field ", name, " in ", q))
    }
    if (s$q != q) yamaa_error("unknown_field", paste0("bad qualifier ", s$q, " for ", q, " records"))
    if (!s$v %in% names(ydf)) yamaa_error("unknown_field", paste0(name, ": no such field"))
    tv(ydf[[s$v]][recs], cts[[s$v]])
  }
  list(resolve = resolve, n = n)
}

apply_multiple_matches <- function(ctx, q, recs, mm) {
  if (is.null(mm) || length(recs) <= 1) return(recs)
  ydf <- ctx$inputs[[q]]
  r <- make_record_resolver(ctx, q, recs)
  ord <- eval_order_terms(mm$order_by, r)  # integer rank vector
  # stable: order by rank, ties by record order
  o <- order(ord, seq_along(ord))
  keep <- mm$keep
  if (is.null(keep) || keep == "first") recs[o[1]] else recs[o[length(o)]]
}

# ---- source expression ----------------------------------------------------
eval_source_expr <- function(payload, ctx) {
  binding <- normalize_source_binding(payload)
  s <- split_qual(binding$variable)
  if (is.null(s$q)) {
    # unqualified: current-output column (REQ-1057)
    # REQ-0148: a filter on a construct with no records to select among --
    # an output column -- fails as prohibited_construct
    if (!is.null(binding$filter))
      yamaa_error("prohibited_construct",
        paste0("source filter does not apply to output column: ", binding$variable))
    return(resolve_name(binding$variable, ctx))
  }
  if (s$q %in% names(ctx$inter_specs)) {
    if (!is.null(binding$filter) || !is.null(binding$multiple_matches))
      yamaa_error("invalid_expression", "source filter/multiple_matches do not apply to intermediates")
    return(inter_read(ctx, s$q, s$v))
  }
  dataset_scalar_read(ctx, s$q, s$v, binding)
}

normalize_source_binding <- function(payload) {
  if (is.character(payload) && length(payload) == 1)
    return(list(variable = payload, filter = NULL, missing = NULL, multiple_matches = NULL))
  if (!is.list(payload) || is.null(payload$variable))
    yamaa_error("invalid_expression", "source needs a variable")
  list(variable = payload$variable, filter = payload$filter,
    missing = payload$missing, multiple_matches = payload$multiple_matches)
}

# ---- literal --------------------------------------------------------------
eval_literal <- function(payload, ctx) {
  literal_to_tv(payload, NULL, ctx$n)
}

# literal_value -> tv of length n; target optionally constrains the type
literal_to_tv <- function(payload, target, n) {
  if (is.null(payload)) return(tv_na(if (!is.null(target)) target else "null", n))
  if (is.logical(payload) && length(payload) == 1) {
    # YAML 1.1 Y/N become booleans; in yamaa they mean the strings "Y"/"N"
    return(tv(rep(if (payload) "Y" else "N", n), "str"))
  }
  if (is.integer(payload) || (is.numeric(payload) && length(payload) == 1)) {
    v <- payload; if (!is.finite(v)) return(tv(rep(NA_real_, n), "float"))
    if (!is.null(target) && target == "float") return(tv(rep(as.numeric(v), n), "float"))
    if (!is.null(target) && target == "int") {
      if (v != floor(v)) yamaa_error("conversion_failed", "literal not integral")
      return(tv(rep(as.integer(v), n), "int"))
    }
    if (is.finite(v) && v == floor(v) && abs(v) < 2^53)  # integral->int, else float
      return(tv(rep(as.integer(v), n), "int"))
    return(tv(rep(as.numeric(v), n), "float"))
  }
  if (is.character(payload) && length(payload) == 1) {
    if (!is.null(target) && target != "str")
      return(convert_tv(tv(rep(payload, n), "str"), target))
    return(tv(rep(payload, n), "str"))
  }
  yamaa_error("invalid_field_type", "bad literal value")
}

# ---- first_available ------------------------------------------------------
eval_first_available <- function(payload, ctx) {
  srcs <- payload$sources
  if (is.null(srcs) || length(srcs) == 0)
    yamaa_error("invalid_expression", "first_available needs sources")
  default <- payload$default
  if (is.null(default)) default <- payload$missing  # tolerate the unified name
  n <- ctx$n
  vals <- lapply(srcs, function(s) {
    b <- normalize_source_binding(s)
    sp <- split_qual(b$variable)
    if (is.null(sp$q)) resolve_name(b$variable, ctx)
    else if (sp$q %in% names(ctx$inter_specs)) inter_read(ctx, sp$q, sp$v)
    else dataset_scalar_read(ctx, sp$q, sp$v, b)
  })
  # result type: first non-null source type
  t <- vals[[1]]$t
  acc <- vals[[1]]$v
  for (k in seq_along(vals)[-1]) {
    vk <- vals[[k]]
    if (vk$t != t) {
      # mixed types: convert to the common type via str? No: fail per strictness.
      # Practical: convert both through the declared column type later; here
      # require equality.
      yamaa_error("incompatible_input_type", "first_available sources disagree in type")
    }
    need <- is.na(acc)
    acc[need] <- vk$v[need]
  }
  out <- tv(acc, t)
  if (!is.null(default)) {
    lit <- literal_to_tv(default, t, n)
    miss <- is.na(out$v)
    out$v[miss] <- lit$v[miss]
  }
  out
}

# ---- greatest / least -----------------------------------------------------
eval_greatest_least <- function(kind, payload, ctx) {
  srcs <- payload$sources
  if (is.null(srcs) || length(srcs) < 2)
    yamaa_error("invalid_expression", paste0(kind, " needs at least two sources"))
  vals <- lapply(srcs, function(s) resolve_name(s, ctx))
  ts <- vapply(vals, function(x) x$t, character(1))
  num <- ts %in% c("int", "float")
  # date/datetime are ISO-8601 strings: lexicographic == chronological (REQ-0003)
  txt <- ts %in% c("str", "date", "datetime")
  if (!(all(num) || all(txt)))
    yamaa_error("incomparable_sources", paste0(kind, " needs all numeric or all str/date"))
  m <- if (kind == "greatest") pmax else pmin
  if (all(num)) {
    xs <- lapply(vals, function(x) as.numeric(x$v))
    all_na <- Reduce(`&`, lapply(xs, is.na))
    r <- suppressWarnings(do.call(m, c(xs, list(na.rm = TRUE))))
    # REQ-1096/1097: largest/smallest non-missing source; missing when none
    r[all_na] <- NA_real_
    t <- if (all(ts == "int")) "int" else "float"
    if (t == "int") return(tv(as.integer(r), "int"))
    return(tv(r, "float"))
  }
  xs <- lapply(vals, function(x) x$v)
  # REQ-0026/REQ-0003: scalar (code-point) order; ISO date/datetime strings
  # sort chronologically under it. R's pmax/pmin use locale collation, so
  # select element-wise instead. Missing unless every source is missing.
  n <- length(xs[[1]])
  r <- rep(NA_character_, n)
  for (i in seq_len(n)) {
    best <- NA_integer_
    for (j in seq_along(xs)) {
      v <- xs[[j]][i]
      if (is.na(v)) next
      if (is.na(best)) { best <- j; next }
      c <- cmp_str_scalar(xs[[best]][i], v)
      if ((kind == "greatest" && c < 0) || (kind == "least" && c > 0)) best <- j
    }
    if (!is.na(best)) r[i] <- xs[[best]][i]
  }
  # retain the (uniform) argument type; mixed date/str falls back to str
  tv(r, if (all(ts == ts[1])) ts[1] else "str")
}

# ---- case -----------------------------------------------------------------
eval_case <- function(payload, ctx) {
  items <- payload
  # payload may be {cases: [...], otherwise: ...} or a bare list
  otherwise <- NULL
  if (is.list(payload) && !is.null(names(payload))) {
    items <- payload$cases; otherwise <- payload$otherwise
  }
  if (is.null(items)) items <- list()
  n <- ctx$n
  out_v <- NULL; out_t <- NULL
  claimed <- rep(FALSE, n)
  resolver <- list(resolve = function(name) resolve_name(name, ctx), n = n)
  for (it in items) {
    if (!is.null(it$otherwise)) {
      # otherwise as a branch item
      otherwise <- it$otherwise
      next
    }
    cond <- eval_pred(parse_predicate_text(it$when), resolver)
    take <- !claimed & !is.na(cond) & cond
    if (any(take)) {
      branch <- eval_expression(it$then, ctx)
      if (is.null(out_t)) { out_t <- branch$t; out_v <- tv_na(out_t, n)$v }
      # null branches coerce to the established type
      if (branch$t == "null") {
        branch <- tv(rep(NA, length(branch$v)), out_t)
      } else if (out_t == "null") {
        out_t <- branch$t
        out_v <- tv_na(out_t, n)$v
        out_v[take] <- branch$v[take]
      } else if (branch$t != out_t) {
        # branches may differ in type (e.g. date vs ISO-date str); convert
        # only the selected values to the established type, failing loudly
        # on genuinely unconvertible values. Stage 3 converts the column.
        # Numeric tower: int and float branches unify to float.
        if (out_t %in% c("int", "float") && branch$t %in% c("int", "float")) {
          out_v <- convert_tv(tv(out_v, out_t), "float")$v
          out_t <- "float"
        }
        conv <- convert_tv(tv(branch$v[take], branch$t), out_t)
        out_v[take] <- conv$v
      } else {
        out_v[take] <- branch$v[take]
      }
      claimed <- claimed | take
    }
  }
  if (is.null(out_t)) {
    # no branch taken and no otherwise: result type unknown; use otherwise or fail
    if (is.null(otherwise)) yamaa_error("invalid_expression", "case with no typed branch")
    branch <- eval_expression(otherwise, ctx)
    return(branch)
  }
  if (!is.null(otherwise)) {
    branch <- eval_expression(otherwise, ctx)
    if (branch$t == "null") {
      branch <- tv(rep(NA, length(branch$v)), out_t)
      out_v[!claimed] <- branch$v[!claimed]
    } else if (out_t == "null") {
      out_t <- branch$t
      out_v <- tv_na(out_t, n)$v
      out_v[!claimed] <- branch$v[!claimed]
    } else if (branch$t != out_t) {
      # numeric tower: int and float branches unify to float
      if (out_t %in% c("int", "float") && branch$t %in% c("int", "float")) {
        out_v <- convert_tv(tv(out_v, out_t), "float")$v
        out_t <- "float"
      }
      conv <- convert_tv(tv(branch$v[!claimed], branch$t), out_t)
      out_v[!claimed] <- conv$v
    } else {
      out_v[!claimed] <- branch$v[!claimed]
    }
  }
  tv(out_v, out_t)
}

# ---- mapping --------------------------------------------------------------
eval_mapping <- function(payload, ctx) {
  src <- eval_mapping_source(payload$source, ctx)
  dict <- payload$dict
  if (is.null(dict) || length(dict) == 0) yamaa_error("invalid_expression", "mapping needs a dict")
  case_sensitive <- if (is.null(payload$case_sensitive)) TRUE else payload$case_sensitive
  # duplicate keys under the comparison rule fail (checked on raw keys)
  # Y/N booleans from YAML 1.1 mean the strings "Y"/"N"
  keys <- vapply(names(dict), function(k)
    if (k == "TRUE") "Y" else if (k == "FALSE") "N" else k, character(1))
  ck <- if (case_sensitive) keys else ascii_upper(keys)
  # REQ-0714: keys colliding after the ASCII fold fail
  if (anyDuplicated(ck)) yamaa_error("ambiguous_dictionary", "mapping dict keys collide")
  # result type from dict values
  vals <- unlist(dict, recursive = FALSE)
  rt <- mapping_value_type(vals)
  n <- ctx$n
  out <- tv_na(rt, n)
  sv <- src$v
  hit <- !is.na(sv)
  strict <- isTRUE(payload$strict)
  # REQ-1110: `missing:` is returned when the source is missing or has no
  # dictionary entry; `strict: true` makes either an `unmapped_value` error
  if (any(hit)) {
    sk <- if (case_sensitive) sv[hit] else ascii_upper(sv[hit])
    # match: dict keys are strings; numeric keys?
    dk <- keys
    m <- match(sk, if (case_sensitive) dk else ascii_upper(dk))
    found <- !is.na(m)
    vv <- mapply(function(mm) dict[[mm]], m[found], USE.NAMES = FALSE)
    out$v[which(hit)[found]] <- coerce_mapping_value(vv, rt)
    unmapped_idx <- which(hit)[!found]
    if (length(unmapped_idx) > 0) {
      if (strict)
        yamaa_error("unmapped_value", "mapping: unmapped value under strict:true")
      if (!"missing" %in% names(payload))
        yamaa_error("unmapped_value", "mapping: unmapped value without policy")
      # explicit `missing: null` keeps the rt-typed NA already in out$v
      if (!is.null(payload$missing)) {
        lit <- literal_to_tv(payload$missing, rt, length(unmapped_idx))
        out$v[unmapped_idx] <- lit$v
      }
    }
  }
  na_idx <- which(is.na(sv))
  if (length(na_idx) > 0) {
    if (strict)
      yamaa_error("unmapped_value", "mapping: missing source under strict:true")
    if (!is.null(payload$missing)) {
      lit <- literal_to_tv(payload$missing, rt, n)
      out$v[na_idx] <- lit$v[na_idx]
    }
  }
  out
}

eval_mapping_source <- function(source, ctx) {
  if (is.character(source) && length(source) == 1) return(resolve_name(source, ctx))
  b <- normalize_source_binding(source)
  s <- split_qual(b$variable)
  if (is.null(s$q)) return(resolve_name(b$variable, ctx))
  if (s$q %in% names(ctx$inter_specs)) return(inter_read(ctx, s$q, s$v))
  dataset_scalar_read(ctx, s$q, s$v, b)
}

mapping_value_type <- function(vals) {
  if (all(vapply(vals, function(x) is.numeric(x) && x == floor(x), logical(1)))) "int"
  else if (all(vapply(vals, is.numeric, logical(1)))) "float"
  else "str"
}

coerce_mapping_value <- function(vv, rt) {
  if (rt == "int") return(as.integer(unlist(vv)))
  if (rt == "float") return(as.numeric(unlist(vv)))
  as.character(unlist(vv))
}

# ---- cut ------------------------------------------------------------------
eval_cut <- function(payload, ctx) {
  src <- eval_mapping_source(payload$source, ctx)
  if (!src$t %in% c("int", "float"))
    yamaa_error("incompatible_input_type", "cut source must be numeric")
  breaks <- as.numeric(payload$breaks)
  labels <- as.character(payload$labels)
  if (length(labels) != length(breaks) + 1)
    yamaa_error("invalid_expression", "cut needs one more label than breaks")
  right <- if (is.null(payload$right)) FALSE else payload$right
  if (anyDuplicated(breaks)) yamaa_error("invalid_expression", "cut breaks must be distinct")
  if (is.unsorted(breaks, strictly = TRUE))
    yamaa_error("invalid_expression", "cut breaks must be strictly increasing")
  n <- ctx$n
  out <- tv_na("str", n)
  xv <- as.numeric(src$v)
  hit <- !is.na(xv)
  if (any(hit)) {
    # findInterval gives the bin index; labels index = bin+1.
    # right=TRUE -> (b[i], b[i+1]] i.e. left.open; right=FALSE -> [b[i], b[i+1]).
    idx <- findInterval(xv[hit], breaks, rightmost.closed = FALSE,
      all.inside = FALSE, left.open = right)
    out$v[which(hit)] <- labels[idx + 1]
  }
  if (!is.null(payload$missing)) {
    out$v[is.na(xv)] <- payload$missing
  }
  out
}

# ---- compute --------------------------------------------------------------
eval_compute_expr <- function(payload, ctx) {
  node <- parse_compute_text(payload$expr)
  resolver <- list(resolve = function(name) {
    # REQ-0442: during column derivation, a qualified identifier whose
    # qualifier is not a declared record lookup (intermediate) fails.
    # Row templates may read input datasets directly.
    s <- split_qual(name)
    if (!is.null(s$q) && ctx$phase == "column" && !s$q %in% names(ctx$inter_specs))
      yamaa_error("qualified_identifier",
        paste0("compute: qualifier is not a declared record lookup: ", name))
    v <- resolve_name(name, ctx)
    if (!v$t %in% c("int", "float"))
      yamaa_error("incompatible_input_type", "compute needs numeric operands")
    v
  }, n = ctx$n)
  eval_compute(node, resolver)
}

# ---- aggregates -----------------------------------------------------------
# Three contexts (REQ-0467): qualified (right-side relation), unqualified
# (current output rows, grouped), grouped input (row template's group).
eval_aggregate <- function(payload, ctx) {
  # aggregate: "<expr text>" shorthand or {expr:, filter:, group_by:, ...}
  if (is.character(payload)) payload <- list(expr = payload)
  expr_text <- payload$expr
  node <- parse_aggregate_text(expr_text)
  quals <- collect_qualifiers(node)
  derive <- payload$derive
  if (!is.null(derive)) {
    # REQ-1191: derive is not available on the grouped-input reduction.
    # A derived reducer names bound variables unqualified, so the reduced
    # relation comes from the bindings and filter (REQ-1191), not the
    # reducer text.
    if (ctx$phase == "row-grouped")
      yamaa_error("invalid_expression", "derive is not available on grouped-row reduction")
    bound_names <- vapply(derive, function(b) b$name, character(1))
    check_derive_mixing(node, bound_names)
    ds <- derive_relation(payload, ctx)
    return(eval_agg_qualified(node, payload, ctx, ds, bound_names))
  }
  if (ctx$phase == "row-grouped") {
    if (length(quals) != 1 || quals[[1]] != ctx$tmpl_ds)
      yamaa_error("invalid_expression", "grouped-row aggregate must name the template's dataset")
    if (!is.null(payload$group_by))
      yamaa_error("invalid_expression", "grouped-row aggregate takes no local group_by")
    recs <- ctx$group_rec
    # recs is a list (one vector per group) in grouped phase
    if (is.list(recs)) {
      out_v <- vector("list", length(recs))
      out_t <- NULL
      for (gi in seq_along(recs)) {
        gr2 <- apply_record_filter(ctx, ctx$tmpl_ds, recs[[gi]], payload$filter)
        val <- eval_agg_over_records(node, ctx, ctx$tmpl_ds, gr2, 1L)
        out_v[[gi]] <- val$v[1]
        if (is.null(out_t)) out_t <- val$t
      }
      return(tv(unlist(out_v), out_t))
    } else {
      recs <- apply_record_filter(ctx, ctx$tmpl_ds, recs, payload$filter)
      return(eval_agg_over_records(node, ctx, ctx$tmpl_ds, recs, ctx$n))
    }
  }
  if (length(quals) == 0) {
    # unqualified: reduce constructed output rows within group_by
    return(eval_agg_unqualified(node, payload, ctx))
  }
  if (length(quals) == 1) {
    return(eval_agg_qualified(node, payload, ctx, quals[[1]]))
  }
  yamaa_error("invalid_expression", "aggregate identifiers must name one relation")
}

collect_qualifiers <- function(node) {
  qs <- character(0)
  walk <- function(x) {
    if (is.list(x)) {
      if (!is.null(x$kind) && x$kind == "id" && !is.null(x$qualifier))
        qs <<- c(qs, x$qualifier)
      if (!is.null(x$kind) && x$kind == "reduce" && !is.null(x$star))
        qs <<- c(qs, x$star)
      for (e in x) walk(e)
    }
  }
  walk(node)
  unique(qs)
}

# evaluate the aggregate grammar over one fixed record set -> scalar tv (len n broadcast)
eval_agg_over_records <- function(node, ctx, ds, recs, n) {
  r <- make_record_resolver(ctx, ds, recs)
  val <- eval_agg_node(node, r)
  if (length(val$v) != 1)
    yamaa_error("invalid_expression", "aggregate must reduce to one value per group")
  tv(rep(val$v, n), val$t)
}

eval_agg_node <- function(node, r) {
  k <- node$kind
  if (k == "reduce") return(eval_reducer(node$reducer, node, r))
  if (k == "lit") {
    n <- r$n
    if (node$vtype == "null") return(tv(NA_real_, 1))
    if (node$vtype == "int") return(tv(as.integer(node$v), "int"))
    return(tv(as.numeric(node$v), "float"))
  }
  if (k == "id") {
    v <- r$resolve(id_to_string(node))
    return(v)
  }
  if (k == "unary") {
    x <- eval_agg_node(node$x, r)
    if (node$op == "+") return(x)
    return(arith_unary_minus(x))
  }
  if (k == "binop") {
    l <- eval_agg_node(node$l, r); rr <- eval_agg_node(node$r, r)
    return(arith_binop(node$op, l, rr))
  }
  if (k == "call") return(eval_num_fn_agg(node$fn, node$args, r))
  yamaa_error("invalid_argument", paste0("bad aggregate node: ", k))
}

eval_num_fn_agg <- function(fn, args, r) {
  ev <- lapply(args, function(a) eval_agg_node(a, r))
  # each arg is now a scalar tv (reductions) -- but a bare identifier arg is
  # still a vector; a call mixing reductions and vectors is an error.
  lens <- vapply(ev, tv_len, integer(1))
  if (any(lens != 1))
    yamaa_error("invalid_expression", "cannot mix reductions and record fields in one call")
  eval_num_fn(fn, ev)
}

# reducers over the resolver's record set -> scalar tv (length 1)
eval_reducer <- function(reducer, node, r) {
  if (!is.null(node$star)) {
    # COUNT(D.*): counts records; NA if no records
    if (reducer != "COUNT") yamaa_error("invalid_expression", "star only for COUNT")
    if (r$n == 0) return(tv_na("int", 1))
    return(tv(as.integer(r$n), "int"))
  }
  x <- eval_agg_node(node$arg, r)
  vals <- x$v
  if (reducer == "COUNT") {
    # COUNT(field): non-missing values; NA if no records
    if (r$n == 0) return(tv_na("int", 1))
    return(tv(sum(!is.na(vals)), "int"))
  }
  v <- vals[!is.na(vals)]
  if (reducer == "ONLY") {
    u <- unique(v)
    if (length(u) == 0) return(tv_na(x$t, 1))
    if (length(u) > 1) yamaa_error("aggregate_multiple_records", "ONLY over differing values")
    return(tv(u[1], x$t))
  }
  # numeric reducers require numeric input; MIN/MAX work for any ordered type
  if (reducer %in% c("SUM", "MEAN")) {
    if (!x$t %in% c("int", "float"))
      yamaa_error("incompatible_input_type", "aggregate operand must be numeric or star-countable")
  }
  if (length(v) == 0) return(tv_na(if (reducer == "MEAN") "float" else x$t, 1))
  num <- as.numeric(v)
  # Use naive summation to match Python's floating point behavior
  # (R's sum() uses long double accumulator)
  naive_sum <- function(x) {
    s <- 0
    for (val in x) s <- s + val
    s
  }
  # MIN/MAX work on the original values (for dates/strings, lexicographic)
  # SUM/MEAN need numeric
  out <- switch(reducer,
    SUM = naive_sum(num),
    MIN = if (x$t %in% c("int", "float")) min(num) else min_scalar_order(v),
    MAX = if (x$t %in% c("int", "float")) max(num) else max_scalar_order(v),
    MEAN = naive_sum(num) / length(num))
  # REQ-0487: SUM retains the argument's numeric type; MIN/MAX retain the
  # argument's type (str for date strings); MEAN is float
  t <- if (reducer == "MEAN") "float" else x$t
  if (t == "int") {
    if (out != floor(out) || abs(out) > .Machine$integer.max)
      yamaa_error("overflow", "integer aggregate overflow")
    return(tv(as.integer(out), "int"))
  }
  tv(out, t)
}

# MIN/MAX over strings/dates in Unicode scalar-value order (REQ-0026).
# R's min/max use locale collation, which is wrong (and NA-prone) here.
min_scalar_order <- function(v) {
  best <- v[1]
  for (x in v[-1]) if (cmp_str_scalar(x, best) < 0) best <- x
  best
}
max_scalar_order <- function(v) {
  best <- v[1]
  for (x in v[-1]) if (cmp_str_scalar(x, best) > 0) best <- x
  best
}

# unqualified aggregate: group current output rows by group_by, reduce, broadcast
eval_agg_unqualified <- function(node, payload, ctx) {
  gb <- payload$group_by
  if (is.null(gb) || length(gb) == 0)
    yamaa_error("invalid_expression", "unqualified aggregate needs group_by")
  n <- ctx$n
  # group assignment for output rows
  gkey <- vapply(seq_len(n), function(i)
    paste(vapply(gb, function(g) canon_key_text(ctx$col[[g]]$t, ctx$col[[g]]$v[i]), character(1)),
      collapse = "\x1f"), character(1))
  out <- NULL; out_t <- NULL
  for (g in unique(gkey)) {
    rows <- which(gkey == g)
    rr <- make_output_row_resolver(ctx, rows)
    recs <- rows
    if (!is.null(payload$filter)) {
      keep <- eval_pred(parse_predicate_text(payload$filter), rr)
      recs <- rows[!is.na(keep) & keep]
      rr <- make_output_row_resolver(ctx, recs)
    }
    val <- eval_agg_node(node, rr)
    if (length(val$v) != 1)
      yamaa_error("invalid_expression", "aggregate must reduce to one value per group")
    if (is.null(out_t)) { out_t <- val$t; out <- tv_na(out_t, n)$v }
    out[rows] <- val$v
  }
  tv(out, out_t)
}

# resolver where identifiers name completed output columns, over a row subset
make_output_row_resolver <- function(ctx, rows) {
  resolve <- function(name) {
    s <- split_qual(name)
    if (!is.null(s$q)) yamaa_error("invalid_expression", "unqualified aggregate takes no qualified names")
    if (!s$v %in% names(ctx$col)) yamaa_error("unknown_field", s$v)
    c <- ctx$col[[s$v]]
    tv(c$v[rows], c$t)
  }
  list(resolve = resolve, n = length(rows))
}

# ---- aggregate derive step (REQ-1189..1192) ----------------------------------
# derive binds per-record typed variables before reduction. Each binding
# evaluates once per record of the aggregate's relation, in declaration
# order; a binding's derivation reads the record's fields (qualified) and
# earlier bindings (unqualified, via ctx$col). Values convert to the
# declared type (REQ-1190; a derive binding carries no `missing:` stage-3
# handler, so a failed conversion is fatal). The reducer names each bound
# variable by its unqualified name (REQ-1189).
eval_derive_bindings <- function(derive, ctx, ds, recs) {
  ydf <- ctx$inputs[[ds]]; cts <- attr(ydf, "coltypes")
  bound <- list()
  for (b in derive) {
    nm <- b$name; tp <- b$type; deriv <- b$derivation
    if (is.null(nm) || is.null(tp) || is.null(deriv) || !tp %in% PUBLIC_TYPES)
      yamaa_error("invalid_expression", "derive binding needs name, type, derivation")
    nrec <- length(recs)
    if (nrec == 0) { bound[[nm]] <- tv_na(tp, 0); next }
    vals <- vector("list", nrec); vt <- NULL
    for (j in seq_len(nrec)) {
      # one-record view of the relation: qualified reads see this record,
      # unqualified reads see only earlier bindings
      ydf1 <- ydf[recs[j], , drop = FALSE]
      attr(ydf1, "coltypes") <- cts
      inputs1 <- ctx$inputs; inputs1[[ds]] <- ydf1
      col1 <- list()
      for (bn in names(bound)) col1[[bn]] <- tv(bound[[bn]]$v[j], bound[[bn]]$t)
      ctx1 <- ctx
      ctx1$n <- 1L; ctx1$inputs <- inputs1; ctx1$col <- col1; ctx1$phase <- "derive"
      v <- eval_expression(deriv, ctx1)
      if (tv_len(v) != 1)
        yamaa_error("invalid_expression",
          paste0("derive binding '", nm, "' must yield one value per record"))
      vals[[j]] <- v$v[1]; if (is.null(vt)) vt <- v$t
    }
    raw <- tv(unlist(vals), vt)
    bound[[nm]] <- apply_declared_type(raw, tp, NULL, FALSE, nm)
  }
  bound
}

# REQ-0468: a reducer mixing a bound variable (unqualified) with a qualified
# identifier is an error. Bound names come from the aggregate's derive list.
check_derive_mixing <- function(node, bound_names) {
  has_bound <- FALSE; has_qual <- FALSE
  walk <- function(x) {
    if (!is.list(x)) return()
    if (!is.null(x$kind) && x$kind == "id") {
      if (is.null(x$qualifier) && x$name %in% bound_names) has_bound <<- TRUE
      if (!is.null(x$qualifier)) has_qual <<- TRUE
    }
    for (e in x) walk(e)
  }
  walk(node)
  if (has_bound && has_qual)
    yamaa_error("invalid_expression",
      "reducer mixes a derive-bound variable with a qualified identifier")
}

# resolver augmented with derive-bound variables: unqualified bound names
# resolve to the bound vectors, everything else to the record resolver
with_derived_bindings <- function(rr, bsub) {
  base_resolve <- rr$resolve
  rr$resolve <- function(name) {
    s <- split_qual(name)
    if (is.null(s$q) && s$v %in% names(bsub)) return(bsub[[s$v]])
    base_resolve(name)
  }
  rr
}

# REQ-1191: the relation a derived aggregate reduces is the one relation
# its derive bindings and filter name; naming two relations is an error.
derive_relation <- function(payload, ctx) {
  qs <- character(0)
  for (b in payload$derive) qs <- c(qs, derive_binding_qualifiers(b$derivation))
  if (!is.null(payload$filter)) {
    fnode <- tryCatch(parse_predicate_text(payload$filter), error = function(e) NULL)
    if (!is.null(fnode)) qs <- c(qs, ast_qualifiers(fnode))
  }
  qs <- unique(qs[qs %in% names(ctx$inputs)])
  if (length(qs) != 1)
    yamaa_error("invalid_expression",
      "derive bindings and filter must name exactly one relation")
  qs[1]
}

# qualifiers named by a derive binding's derivation (skips literal leaves)
derive_binding_qualifiers <- function(deriv) {
  qs <- character(0)
  walk <- function(x, key = NULL) {
    if (identical(key, "literal")) return()
    if (is.character(x) && length(x) == 1) {
      s <- split_qual(x)
      if (!is.null(s$q)) qs <<- c(qs, s$q)
      return()
    }
    if (is.list(x)) {
      nm <- names(x)
      for (i in seq_along(x)) walk(x[[i]], if (!is.null(nm)) nm[i] else NULL)
    }
  }
  walk(deriv)
  unique(qs)
}

# qualifiers named by identifier nodes in a parsed predicate/aggregate AST
ast_qualifiers <- function(node) {
  qs <- character(0)
  walk <- function(x) {
    if (!is.list(x)) return()
    if (identical(x$kind, "id") && !is.null(x$qualifier)) qs <<- c(qs, x$qualifier)
    for (e in x) walk(e)
  }
  walk(node)
  unique(qs)
}

# qualified aggregate: reduce right-side dataset D per row, join on key
# REQ-0503: walk an aggregate AST; any identifier outside a reduction that
# is not a group_by column fails with aggregate_identifier_not_grouped
check_agg_grouped_ids <- function(node, gb, ds, bound_names = character(0)) {
  gb_full <- gb  # qualified group_by terms, e.g. "EX.STUDYID"
  walk <- function(x, in_reduce) {
    if (!is.list(x)) return()
    if (!is.null(x$kind) && x$kind == "reduce") {
      # identifiers inside a reduction are fine; walk children as in-reduce
      for (e in x) walk(e, TRUE)
      return()
    }
    if (!is.null(x$kind) && x$kind == "id" && !in_reduce) {
      # reconstruct qualified name
      nm <- if (!is.null(x$qualifier)) paste0(x$qualifier, ".", x$name) else x$name
      # derive-bound variables are per-record values, not group_by columns
      if (is.null(x$qualifier) && nm %in% bound_names) return()
      if (!nm %in% gb_full)
        yamaa_error("aggregate_identifier_not_grouped",
          paste0("aggregate identifier not a group_by column: ", nm))
    }
    for (e in x) walk(e, in_reduce)
  }
  walk(node, FALSE)
}

eval_agg_qualified <- function(node, payload, ctx, ds, bound_names = character(0)) {
  ydf <- ctx$inputs[[ds]]
  cts <- attr(ydf, "coltypes")
  n <- ctx$n
  gb <- payload$group_by
  key <- payload$key
  if (is.null(gb)) {
    # default: the applicable keys (REQ-0150 inference), or the explicit
    # key columns if key is given (group by what we join on)
    if (!is.null(key)) {
      gb <- paste0(ds, ".", key)
    } else {
      ak <- intersect(ctx$keys, names(ydf))
      if (length(ak) == 0) yamaa_error("unjoinable", paste0("no shared key with ", ds))
      gb <- paste0(ds, ".", ak)
      key <- ak
    }
  }
  gb_cols <- vapply(gb, function(g) split_qual(g)$v, character(1))
  # check qualifiers
  for (g in gb) if (split_qual(g)$q != ds)
    yamaa_error("invalid_expression", "aggregate group_by must name the right-side dataset")
  # REQ-0503: an identifier outside a reduction that is not a group_by
  # column fails -- checked before key/group_by length validation.
  # Derive-bound variables are exempt: they name per-record values.
  derive <- payload$derive
  check_agg_grouped_ids(node, gb, ds, bound_names)
  if (is.null(key)) key <- gb_cols  # same names on the current row
  if (length(key) != length(gb_cols)) yamaa_error("invalid_expression", "key/group_by length mismatch")
  # eligible right-side records (filter once)
  all_recs <- seq_len(nrow(ydf))
  all_recs <- apply_record_filter(ctx, ds, all_recs, payload$filter)
  # REQ-1189: the derive bindings evaluate once per record of the relation
  bound <- if (is.null(derive)) NULL else eval_derive_bindings(derive, ctx, ds, all_recs)
  # group eligible records by gb_cols
  grp <- build_group_index(ctx, ds, gb_cols, all_recs)
  # between narrowing is per row
  has_between <- !is.null(payload$between)
  if (has_between) {
    bw <- payload$between
    if (is.null(bw$value) || (is.null(bw$lower) && is.null(bw$upper)))
      yamaa_error("invalid_expression", "between needs value and a bound")
    if (!is.null(bw$lower) && split_qual(bw$lower)$q != ds) yamaa_error("invalid_expression", "between lower must name the dataset")
    if (!is.null(bw$upper) && split_qual(bw$upper)$q != ds) yamaa_error("invalid_expression", "between upper must name the dataset")
  }
  out <- NULL; out_t <- NULL
  for (i in seq_len(n)) {
    # find the group matching this row's key values
    kv <- vapply(seq_along(gb_cols), function(j)
      canon_key_text(ctx$col[[key[j]]]$t, ctx$col[[key[j]]]$v[i]), character(1))
    recs <- grp[[paste(kv, collapse = "\x1f")]]
    if (is.null(recs)) recs <- integer(0)
    if (has_between && length(recs) > 0) {
      vv <- ctx$col[[bw$value]]  # current-row variable
      recs <- narrow_between(ydf, cts, recs, vv$v[i], vv$t, bw, ds)
    }
    rr <- make_record_resolver(ctx, ds, recs)
    if (!is.null(bound)) {
      idx <- match(recs, all_recs)
      bsub <- lapply(bound, function(bv) tv(bv$v[idx], bv$t))
      rr <- with_derived_bindings(rr, bsub)
    }
    val <- eval_agg_node(node, rr)
    if (length(val$v) != 1)
      yamaa_error("invalid_expression", "aggregate must reduce to one value per group")
    if (is.null(out_t)) { out_t <- val$t; out <- tv_na(out_t, n)$v }
    out[i] <- val$v
  }
  tv(out, out_t)
}

# group a record subset by columns -> env key -> int vector
build_group_index <- function(ctx, ds, cols, recs) {
  ydf <- ctx$inputs[[ds]]; cts <- attr(ydf, "coltypes")
  env <- new.env(hash = TRUE, parent = emptyenv())
  if (length(recs) == 0) return(env)
  kv <- vapply(recs, function(r)
    paste(vapply(cols, function(k) canon_key_text(cts[[k]], ydf[[k]][r]), character(1)),
      collapse = "\x1f"), character(1))
  sp <- split(recs, kv)
  for (kk in names(sp)) env[[kk]] <- sp[[kk]]
  env
}

narrow_between <- function(ydf, cts, recs, value, vt, bw, ds) {
  keep <- rep(TRUE, length(recs))
  if (!is.null(bw$lower)) {
    lc <- split_qual(bw$lower)$v
    lv <- ydf[[lc]][recs]; lt <- cts[[lc]]
    c <- cmp_typed(tv(rep(value, length(recs)), vt), tv(lv, lt))
    keep <- keep & !is.na(c) & c >= 0
  }
  if (!is.null(bw$upper)) {
    uc <- split_qual(bw$upper)$v
    uv <- ydf[[uc]][recs]; ut <- cts[[uc]]
    c <- cmp_typed(tv(rep(value, length(recs)), vt), tv(uv, ut))
    keep <- keep & !is.na(c) & c <= 0
  }
  recs[keep]
}

# ---- inline lookup --------------------------------------------------------
eval_lookup_inline <- function(payload, ctx) {
  ds <- payload$dataset
  if (is.null(ds) || !ds %in% names(ctx$inputs))
    yamaa_error("unknown_field", paste0("lookup dataset: ", ds))
  ydf <- ctx$inputs[[ds]]; cts <- attr(ydf, "coltypes")
  n <- ctx$n
  key_base <- payload$key_base; key <- payload$key
  if (is.null(key)) {
    ak <- intersect(ctx$keys, names(ydf))
    if (length(ak) == 0) yamaa_error("unjoinable", paste0("lookup: no shared key with ", ds))
    key <- ak
  }
  if (is.null(key_base)) key_base <- key
  key_base <- as.character(key_base); key <- as.character(key)
  if (length(key_base) != length(key))
    yamaa_error("source_key_length_mismatch", "lookup key_base/key length mismatch")
  value_col <- payload$value
  if (is.null(value_col) || !value_col %in% names(ydf))
    yamaa_error("unknown_field", paste0("lookup value: ", value_col))
  # eligible records
  recs <- seq_len(nrow(ydf))
  recs <- apply_record_filter(ctx, ds, recs, payload$filter)
  # between narrowing per row happens below
  idx <- build_group_index(ctx, ds, key, recs)
  t <- cts[[value_col]]
  out <- tv_na(t, n)
  for (i in seq_len(n)) {
    kv <- vapply(seq_along(key), function(j) {
      bv <- resolve_name(key_base[j], ctx)
      canon_key_text(bv$t, bv$v[i])
    }, character(1))
    m <- idx[[paste(kv, collapse = "\x1f")]]
    if (is.null(m)) m <- integer(0)
    if (!is.null(payload$between) && length(m) > 0) {
      bw <- payload$between
      bv <- resolve_name(bw$value, ctx)
      m <- narrow_between(ydf, cts, m, bv$v[i], bv$t, bw, ds)
    }
    if (!is.null(payload$order_by) && length(m) > 1) {
      r <- make_record_resolver(ctx, ds, m)
      ord <- eval_order_terms(payload$order_by, r)
      o <- order(ord, seq_along(ord))
      m <- m[if (payload$keep == "last") o[length(o)] else o[1]]
    }
    if (length(m) == 0) {
      if (!is.null(payload$missing)) {
        out$v[i] <- literal_to_tv(payload$missing, t, 1)$v
      } else if (isTRUE(payload$strict)) {
        yamaa_error("unmatched_key", "strict lookup found nothing")
      }
      next
    }
    if (length(m) > 1) yamaa_error("multiple_matches", "lookup matched multiple records")
    out$v[i] <- ydf[[value_col]][m]
  }
  out
}

# ---- temporal expressions -------------------------------------------------
eval_to_date <- function(payload, ctx) {
  op_to_date(resolve_name(payload$source, ctx))
}

eval_to_epoch_day <- function(payload, ctx) {
  op_to_epoch_day(resolve_name(payload$source, ctx))
}

eval_date_diff <- function(payload, ctx) {
  s <- resolve_name(payload$start, ctx); e <- resolve_name(payload$end, ctx)
  # REQ-0606: date operations reject datetime
  if (s$t == "datetime" || e$t == "datetime")
    yamaa_error("incompatible_input_type", "date_diff needs dates, not datetimes")
  if (s$t != "date" || e$t != "date") {
    # allow str sources convertible to date
    s <- convert_tv(s, "date"); e <- convert_tv(e, "date")
  }
  tv(op_date_diff(s$v, e$v,
    if (is.null(payload$unit)) "day" else payload$unit,
    if (is.null(payload$bounds)) "exclusive" else payload$bounds), "int")
}

eval_study_day <- function(payload, ctx) {
  d <- resolve_name(payload$date, ctx); r <- resolve_name(payload$reference, ctx)
  # REQ-0606: date operations (other than to_date) reject datetime
  if (d$t == "datetime" || r$t == "datetime")
    yamaa_error("incompatible_input_type", "study_day needs dates, not datetimes")
  if (d$t != "date") d <- convert_tv(d, "date")
  if (r$t != "date") r <- convert_tv(r, "date")
  tv(op_study_day(d$v, r$v), "int")
}

eval_date_impute <- function(payload, ctx) {
  src <- resolve_name(payload$source, ctx)
  if (src$t != "str") yamaa_error("incompatible_input_type", "date_impute source must be str")
  # REQ-0608: month must be 1..12 (validation phase)
  mo <- as.integer(payload$month)
  if (!is.na(mo) && (mo < 1 || mo > 12))
    yamaa_error("month_out_of_range", paste0("month out of range: ", payload$month))
  n <- ctx$n
  month <- rep(as.integer(payload$month), n)
  day <- payload$day
  dayv <- if (is.character(day)) rep(day, n) else rep(as.integer(day), n)
  nb <- if (!is.null(payload$not_before)) {
    x <- resolve_name(payload$not_before, ctx)
    if (x$t == "datetime") x <- tv(substr(x$v, 1, 10), "date")  # day-precision bound
    if (x$t != "date") x <- convert_tv(x, "date")
    x$v
  } else NULL
  op_date_impute(src, as.list(month), as.list(dayv),
    if (is.null(payload$minimum_source_precision)) "year" else payload$minimum_source_precision,
    nb,
    if (is.null(payload$missing)) NA_character_ else payload$missing,
    # REQ-0588: absent invalid -> fail; explicit null -> NA
    if ("invalid" %in% names(payload)) {
      ih <- payload$invalid
      if (is.null(ih)) NA_character_ else ih
    } else "ABSENT",
    ctx)
}

eval_date_precision <- function(payload, ctx) {
  src <- resolve_name(payload$source, ctx)
  mh <- if (is.null(payload$missing)) NA_character_ else payload$missing
  # REQ-0588: no `invalid` handler + non-date text fails invalid_date_text;
  # pass NULL (not NA) so op_date_precision can distinguish
  ih <- payload$invalid
  tv(op_date_precision(src, mh, ih)$v, "str")
}

# REQ-1182: datetime_impute completes a date to a datetime at the declared
# day edge, or passes a complete datetime through with precision "second".
# A source truncated before its day is invalid (no second imputation
# policy). REQ-0609/1184: a `time` that is neither first nor last is
# rejected where the specification is read, before any data is seen.
eval_datetime_impute <- function(payload, ctx) {
  if (is.null(payload$time) || !payload$time %in% c("first", "last"))
    yamaa_error("value_not_permitted",
      paste0("datetime_impute time must be first or last: ", payload$time))
  src <- resolve_name(payload$source, ctx)
  op_datetime_impute(src, payload$time,
    if (is.null(payload$missing)) NA_character_ else payload$missing,
    # REQ-0588 pattern: absent invalid -> fail; explicit null -> NA
    if ("invalid" %in% names(payload)) {
      ih <- payload$invalid
      if (is.null(ih)) NA_character_ else ih
    } else "ABSENT")
}

# REQ-1183: "S" when the source carried a time, "D" when datetime_impute
# supplied it. A datetime value binds the answer to the completed value.
eval_datetime_precision <- function(payload, ctx) {
  src <- resolve_name(payload$source, ctx)
  mh <- if (is.null(payload$missing)) NA_character_ else payload$missing
  # REQ-0588 pattern: pass NULL (not NA) so op_datetime_precision can
  # distinguish an absent `invalid` handler (fail) from a declared one
  ih <- payload$invalid
  tv(op_datetime_precision(src, mh, ih)$v, "str")
}

# ---- str expressions ------------------------------------------------------
eval_str_case <- function(kind, payload, ctx) {
  if (!is.character(payload$source))
    yamaa_error("invalid_field_type",
      paste0(kind, " source must be a field reference"))
  out <- op_str_case(resolve_name(payload$source, ctx), kind == "str_upper")
  if (!is.null(payload$missing)) {
    out$v[is.na(out$v)] <- payload$missing
  }
  out
}

eval_str_extract <- function(payload, ctx) {
  op_str_extract(resolve_name(payload$source, ctx), payload$pattern,
    if (is.null(payload$group)) 0L else as.integer(payload$group),
    if (is.null(payload$missing)) NA_character_ else payload$missing,
    if (is.null(payload$invalid)) NA_character_ else payload$invalid,
    if (is.null(payload$no_match)) NA_character_ else payload$no_match)
}

eval_str_concat <- function(payload, ctx) {
  # sources is list[expression]: each may be a bare variable, {source:},
  # {literal:}, or any scalar expression
  srcs <- lapply(payload$sources, function(s) eval_expression(s, ctx))
  op_str_concat(srcs,
    if (is.null(payload$separator)) "" else payload$separator,
    if (is.null(payload$missing)) NA_character_ else payload$missing)
}

eval_str_template <- function(payload, ctx) {
  # str_template: may be a bare string or the str_template_class map
  if (is.character(payload)) payload <- list(template = payload)
  # parse (and validate) the template BEFORE binding placeholders, so an
  # invalid template fails invalid_string_template (REQ-0461) rather than
  # crashing on argument resolution
  parts <- parse_string_template(payload$template)
  names_needed <- template_placeholders(parts)
  args <- lapply(names_needed, function(nm) resolve_name(nm, ctx))
  names(args) <- names_needed
  # Also include explicit arguments if present (override)
  if (!is.null(payload$arguments)) {
    exp_args <- lapply(payload$arguments, function(a) resolve_name(a, ctx))
    # payload$arguments is a list; names may be in the spec
    for (nm in names(exp_args)) args[[nm]] <- exp_args[[nm]]
  }
  op_str_template(parts, args,
    if (is.null(payload$missing)) NA_character_ else payload$missing,
    if (is.null(payload$invalid)) NA_character_ else payload$invalid,
    ctx$n)
}

eval_round_half <- function(payload, ctx) {
  src <- resolve_name(payload$source, ctx)
  digits <- as.integer(payload$digits)
  if (is.null(payload$digits)) digits <- 0L
  round_half_away(src, digits)
}

# ---- project functions ------------------------------------------------------
# a scalar yaml value (contract default) as a constant tv
fn_scalar_tv <- function(x, n) {
  if (is.null(x)) return(tv_na("null", n))
  if (is.numeric(x) && length(x) == 1)
    return(tv(rep(x, n), if (x == floor(x) && abs(x) <= .Machine$integer.max) "int" else "float"))
  if (is.logical(x) && length(x) == 1) return(tv(rep(x, n), "bool"))
  if (is.character(x) && length(x) == 1) return(tv(rep(x, n), "str"))
  yamaa_error("invalid_function_argument", "unsupported function argument")
}

fn_arg_to_tv <- function(a, n, ctx) {
  if (is.list(a) && !is.null(a$literal)) return(tv(rep(as.character(a$literal), n), "str"))
  if (is.list(a) && !is.null(a$date)) return(tv(rep(as.character(a$date), n), "date"))
  if (is.list(a) && !is.null(a$datetime)) return(tv(rep(as.character(a$datetime), n), "datetime"))
  if (is.character(a) && length(a) == 1) return(resolve_name(a, ctx))
  fn_scalar_tv(a, n)
}

# argument tv type against the declared param type; missing ("null") always
# passes -- per-row missing behavior is governed by accepts_missing
fn_arg_type_ok <- function(at, ptype) {
  if (at == "null") return(TRUE)
  if (at == ptype) return(TRUE)
  at == "int" && ptype == "float"  # numeric tower
}

# an invoked binding's scalar against the declared returns type
# (missing is checked separately via may_return_missing)
fn_result_type_ok <- function(r, rtype) {
  if (is.na(r)) return(TRUE)
  switch(rtype,
    float = is.numeric(r),
    int = is.numeric(r) && r == floor(r),
    str = is.character(r),
    bool = is.logical(r),
    date = is.character(r),
    datetime = is.character(r),
    FALSE)
}

eval_function_expr <- function(payload, ctx) {
  name <- payload$name
  pe <- ctx$project_env
  entry <- if (!is.null(pe)) pe$env$functions[[name]] else NULL
  if (is.null(entry))
    yamaa_error("unknown_project_function", paste0("no project function: ", name))
  call_str <- entry$binding$call
  if (is.null(call_str))
    yamaa_error("project_environment_invalid",
      paste0("function '", name, "' has no binding.call"))
  fn <- resolve_project_callable(call_str, ctx$project_fns)
  contract <- function_contract_fields(pe, name)
  params <- contract$params
  if (is.null(params)) params <- list()

  # ---- structural argument checks (REQ-0700), once per call ----
  args_spec <- payload$args
  if (is.null(args_spec)) args_spec <- list()
  supplied <- names(args_spec)
  if (is.null(supplied)) supplied <- character(0)
  pnames <- vapply(params, function(p) as.character(p[["name"]]), character(1))
  unknown <- setdiff(supplied, pnames)
  if (length(unknown) > 0)
    yamaa_error("invalid_function_argument",
      paste0("unknown argument(s) for '", name, "': ", paste(unknown, collapse = ", ")))
  n <- ctx$n
  pargs <- list()
  for (p in params) {
    pn <- as.character(p[["name"]])
    ptype <- as.character(p[["type"]])
    required <- if (is.null(p[["required"]])) TRUE else isTRUE(p[["required"]])
    accepts <- if (is.null(p[["accepts_missing"]])) FALSE else isTRUE(p[["accepts_missing"]])
    if (pn %in% supplied) {
      at <- fn_arg_to_tv(args_spec[[pn]], n, ctx)
      if (!fn_arg_type_ok(at$t, ptype))
        yamaa_error("invalid_function_argument", paste0("argument '", pn,
          "' has type ", at$t, "; contract wants ", ptype))
    } else if (!is.null(p[["default"]])) {
      # REQ-0681: omitting an optional argument selects the default
      at <- fn_scalar_tv(p[["default"]], n)
      if (!fn_arg_type_ok(at$t, ptype))
        yamaa_error("invalid_function_argument", paste0("default for '", pn,
          "' has type ", at$t, "; contract wants ", ptype))
    } else if (required) {
      yamaa_error("invalid_function_argument",
        paste0("missing required argument '", pn, "' for '", name, "'"))
    } else {
      next  # optional without default: omitted; the host default applies
    }
    pargs[[pn]] <- list(tv = at, accepts_missing = accepts)
  }
  # REQ-1084: binding.args maps logical to host argument names
  argmap <- entry$binding$args
  host_names <- vapply(names(pargs), function(pn)
    if (!is.null(argmap) && !is.null(argmap[[pn]])) as.character(argmap[[pn]]) else pn,
    character(1))

  may_missing <- isTRUE(contract[["may_return_missing"]])
  rtype <- contract[["returns"]]

  out <- vector("list", n)
  for (i in seq_len(n)) {
    # REQ-0681: a missing value for a non-accepting parameter short-circuits
    # the row (no call); an accepting parameter gets the canonical missing
    sc <- FALSE
    call_args <- list()
    for (pn in names(pargs)) {
      pa <- pargs[[pn]]
      v <- pa$tv$v[i]
      if (is.na(v) && !pa$accepts_missing) { sc <- TRUE; break }
      call_args[[pn]] <- v
    }
    if (sc) { out[[i]] <- NA; next }
    names(call_args) <- host_names
    r <- tryCatch(do.call(fn, call_args),
      error = function(e) yamaa_error("function_call_failed",
        paste0("project function '", name, "' failed: ", conditionMessage(e))))
    # REQ-0686: non-finite normalization runs immediately after the host
    # scalar returns, before the result checks
    if (is.numeric(r) && length(r) == 1 && !is.na(r) && !is.finite(r)) r <- NA_real_
    # REQ-0702: wrong shape/type, or missing without may_return_missing
    if (length(r) != 1)
      yamaa_error("invalid_function_result",
        paste0("project function '", name, "' must return one scalar per row"))
    if (!is.null(rtype) && !fn_result_type_ok(r, as.character(rtype)))
      yamaa_error("invalid_function_result",
        paste0("project function '", name, "' returned the wrong type"))
    if (is.na(r) && !may_missing)
      yamaa_error("invalid_function_result",
        paste0("project function '", name, "' returned missing without may_return_missing"))
    out[[i]] <- r
  }
  flat <- unlist(out, recursive = FALSE)
  # infer the result type from non-missing values; all-missing falls back to
  # float (converts cleanly to any declared column type)
  nn <- flat[!is.na(flat)]
  t <- if (length(nn) == 0) "float"
    else if (is.numeric(nn) && all(nn == floor(nn))) "int"
    else if (is.numeric(nn)) "float"
    else if (is.character(nn)) "str"
    else if (is.logical(nn)) "bool" else "str"
  if (length(nn) == 0) flat <- rep(NA_real_, n)
  tv(flat, t)
}
