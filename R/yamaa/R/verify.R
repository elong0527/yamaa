# verify.R -- verification (rules/execution/verification.md).
#
# run_column_verifications(cspec, ctx) and run_dataset_verifications(spec, ctx)
# raise yamaa_error("verification_failed", ...) on the first error-level
# failure. REQ-0389/0390: severity defaults to error; warning violations do
# not fail -- they accumulate in ctx$warn_env$entries in check order.

run_column_verifications <- function(cspec, ctx) {
  nm <- cspec$name
  c <- ctx$col[[nm]]
  verifs <- normalize_verifications(cspec$verifications)
  prefix <- paste0("columns.", nm, ".verifications")
  for (i in seq_along(verifs))
    verify_column_one(nm, c, verifs[[i]], ctx, i - 1L, prefix)
}

# stable condition + requirement per verification kind (yaml/conditions.yaml,
# rules/execution/verification.md REQ-0375..REQ-0385)
verif_meta <- function(kind) {
  m <- list(
    not_missing = c("not_missing_failed", "REQ-0375"),
    allowed_values = c("allowed_values_failed", "REQ-0376"),
    range = c("range_failed", "REQ-0377"),
    max_length = c("length_failed", "REQ-0378"),
    matches = c("matches_failed", "REQ-0379"),
    unique = c("unique_failed", "REQ-0381"),
    row_count = c("row_count_failed", "REQ-0385"),
    assert = c("assert_failed", "REQ-0384"),
    implies = c("implication_failed", "REQ-0383"),
    all_or_none = c("all_or_none_failed", "REQ-0382"))
  if (!kind %in% names(m))
    yamaa_error("invalid_spec", paste0("unknown verification: ", kind))
  list(condition = m[[kind]][1], requirement = m[[kind]][2])
}

verify_column_one <- function(nm, c, v, ctx, idx0, prefix) {
  kind <- names(v)[1]
  p <- v[[1]]
  meta <- verif_meta(kind)
  sev <- p$severity
  if (is.null(sev)) sev <- "error"
  if (!sev %in% c("error", "warning"))
    yamaa_error("invalid_spec", paste0("unknown severity: ", sev))
  spec_path <- paste0(prefix, "[", idx0, "].", kind)
  # REQ-1173..1178: every evaluated check is recorded (held or violated),
  # in execution order; warning violations additionally log a violation.
  report <- function(msg, bad, details, evaluated, cond = meta$condition) {
    n_bad <- sum(bad)
    record_check(ctx, list(spec_path = spec_path,
      verification_id = if (is.null(p$id)) NA_character_ else as.character(p$id),
      check = kind, target = nm, requirement = meta$requirement,
      severity = sev, outcome = if (n_bad > 0) "violated" else "held",
      condition = if (n_bad > 0) cond else NA_character_,
      evaluated_count = as.integer(evaluated), failure_count = as.integer(n_bad),
      details = if (n_bad > 0) json_write(details) else "{}"))
    if (n_bad == 0) return(invisible(NULL))
    if (sev == "warning") {
      log_warning(ctx, list(
        condition = cond, requirement = meta$requirement,
        spec_path = spec_path, verification_id = p$id,
        failure_count = n_bad,
        offending_keys = offending_keys_json(ctx, which(bad), ctx$keys),
        details = json_write(details)))
    } else {
      yamaa_error(cond,
        paste0("column ", nm, " ", kind, ": ", msg))
    }
  }
  vals <- c$v
  present <- !is.na(vals)
  details <- list(column = nm)
  if (kind == "not_missing") {
    report("missing values present", !present, details, length(vals))
  } else if (kind == "allowed_values") {
    vals_list <- lapply(p$values, function(x) {
      # Y/N booleans from YAML 1.1 mean the strings "Y"/"N"
      if (is.logical(x) && length(x) == 1 && !is.na(x)) if (x) "Y" else "N" else x
    })
    allowed <- literal_list_tv(vals_list, c$t)
    bad <- present & vapply(vals,
      function(x) !any(eq_with_na(x, allowed$v)), logical(1))
    report(paste0(sum(bad), " values outside the allowed set"), bad, details, length(vals))
  } else if (kind == "range") {
    if (c$t != "float" && c$t != "int") {
      report("range on non-numeric column", rep(TRUE, length(vals)), details,
        length(vals),
        cond = if (sev == "warning") "incompatible_input_type" else meta$condition)
      return(invisible(NULL))
    }
    lo <- p$min; hi <- p$max
    bad <- rep(FALSE, length(vals))
    if (!is.null(lo)) bad <- bad | (present & vals < lo)
    if (!is.null(hi)) bad <- bad | (present & vals > hi)
    msg <- if (!is.null(lo) && any(present & vals < lo)) "value below min"
           else if (any(bad)) "value above max" else ""
    report(msg, bad, details, length(vals))
  } else if (kind == "matches") {
    if (c$t != "str")
      yamaa_error(meta$condition,
        paste0("column ", nm, " ", kind, ": matches on non-string column"))
    rx <- p$pattern
    # REQ-0827: invalid regex fails at validation
    assert_valid_regex(rx)
    bad <- present & !vapply(vals, function(x) grepl(rx, x, perl = TRUE), logical(1))
    report("value does not match pattern", bad, details, length(vals))
  } else if (kind == "max_length") {
    if (c$t != "str")
      yamaa_error(meta$condition,
        paste0("column ", nm, " ", kind, ": max_length on non-string column"))
    bad <- present & nchar(vals) > p$max
    report("value exceeds max_length", bad, details, length(vals))
  } else {
    yamaa_error("invalid_spec", paste0("unknown column verification: ", kind))
  }
}

# verifications can be a named list (kind -> payload) or a list of
# single-key maps; normalize to a list of single-key named lists.
normalize_verifications <- function(v) {
  if (is.null(v) || length(v) == 0) return(list())
  if (!is.null(names(v)) && any(names(v) != "")) {
    # named list: split into single-key lists
    lapply(seq_along(v), function(i) v[i])
  } else {
    # already a list of single-key maps
    v
  }
}

eq_with_na <- function(x, allowed) {
  # TRUE where allowed equals x, treating NA==NA as equal
  vapply(allowed, function(a) (is.na(x) && is.na(a)) || (!is.na(x) && !is.na(a) && x == a),
    logical(1))
}

# REQ-0390: warning violations accumulate in check order in an environment
# (so the log survives ctx's copy-on-modify semantics).
log_warning <- function(ctx, entry) {
  if (is.null(ctx$warn_env)) return(invisible(NULL))
  ctx$warn_env$entries[[length(ctx$warn_env$entries) + 1L]] <- entry
  invisible(NULL)
}

# REQ-1173..1178: every evaluated check is recorded for the verification
# report (held or violated), in execution order, in ctx$check_env$rows.
record_check <- function(ctx, entry) {
  if (is.null(ctx$check_env)) return(invisible(NULL))
  ctx$check_env$rows[[length(ctx$check_env$rows) + 1L]] <- entry
  invisible(NULL)
}

literal_list_tv <- function(lits, t) {
  tvs <- lapply(lits, function(l) literal_to_tv(l, t, 1L)$v)
  tv(unlist(tvs), t)
}

run_dataset_verifications <- function(spec, ctx) {
  if (is.null(spec$verifications)) return(invisible(NULL))
  verifs <- normalize_verifications(spec$verifications)
  for (i in seq_along(verifs)) {
    v <- verifs[[i]]
    p <- v[[1]]
    kind <- names(v)[1]
    # REQ-0374: an id is required only for all_or_none, implies, assert,
    # and row_count with group_by (unique across those that declare them)
    if (is.null(p$id) && (kind %in% c("all_or_none", "implies", "assert") ||
        (kind == "row_count" && !is.null(p$group_by))))
      yamaa_error("missing_verification_id",
        paste0("verification ", kind, " has no id"))
    verify_dataset_one(v, ctx, i - 1L)
  }
}

verify_dataset_one <- function(v, ctx, idx0) {
  kind <- names(v)[1]
  p <- v[[1]]
  meta <- verif_meta(kind)
  id <- if (!is.null(p$id)) paste0("[", p$id, "] ") else ""
  sev <- p$severity
  if (is.null(sev)) sev <- "error"
  if (!sev %in% c("error", "warning"))
    yamaa_error("invalid_spec", paste0("unknown severity: ", sev))
  spec_path <- paste0("verifications[", idx0, "].", kind)
  # REQ-1173..1178: every evaluated check is recorded (held or violated),
  # in execution order; warning violations additionally log a violation.
  report <- function(msg, bad_rows, keynames, details, evaluated) {
    n_bad <- length(bad_rows)
    record_check(ctx, list(spec_path = spec_path,
      verification_id = if (is.null(p$id)) NA_character_ else as.character(p$id),
      check = kind, target = NA_character_, requirement = meta$requirement,
      severity = sev, outcome = if (n_bad > 0) "violated" else "held",
      condition = if (n_bad > 0) meta$condition else NA_character_,
      evaluated_count = as.integer(evaluated), failure_count = as.integer(n_bad),
      details = if (n_bad > 0) json_write(details) else "{}"))
    if (n_bad == 0) return(invisible(NULL))
    if (sev == "warning") {
      log_warning(ctx, list(
        condition = meta$condition, requirement = meta$requirement,
        spec_path = spec_path, verification_id = p$id,
        failure_count = n_bad,
        offending_keys = offending_keys_json(ctx, bad_rows, keynames),
        details = json_write(details)))
    } else {
      yamaa_error(meta$condition, paste0(id, kind, ": ", msg))
    }
  }
  n <- ctx$n
  resolver_of <- function() {
    list(resolve = function(name) {
      s <- split_qual(name)
      if (!is.null(s$q) || !s$v %in% names(ctx$col))
        yamaa_error("unknown_field", name)
      ctx$col[[s$v]]
    }, n = n)
  }
  if (kind == "unique") {
    cols <- p$columns
    key <- vapply(seq_len(n), function(i)
      paste(vapply(cols, function(k) {
        c <- ctx$col[[k]]; canon_key_text(c$t, c$v[i])
      }, character(1)), collapse = "\x1f"), character(1))
    dups <- duplicated(key) | duplicated(key, fromLast = TRUE)
    report("duplicate key combination", which(dups), cols,
      list(columns = as.list(cols)), length(unique(key)))
  } else if (kind == "row_count") {
    if (!is.null(p$group_by)) {
      # grouped row count requires an id (for error reporting)
      if (is.null(p$id) || nchar(p$id) == 0)
        yamaa_error("invalid_spec", "grouped row_count verification requires an id")
      gb_cols <- p$group_by
      rows <- seq_len(n)
      if (!is.null(p$filter)) {
        f <- eval_pred(parse_predicate_text(p$filter), resolver_of())
        rows <- rows[!is.na(f) & f]
      }
      counts <- integer(0); firsts <- integer(0)
      if (length(rows) > 0) {
        gkey <- vapply(rows, function(i)
          paste(vapply(gb_cols, function(k) {
            c <- ctx$col[[k]]; canon_key_text(c$t, c$v[i])
          }, character(1)), collapse = "\x1f"), character(1))
        ug <- unique(gkey)
        firsts <- vapply(ug, function(g) rows[which(gkey == g)[1L]], integer(1))
        counts <- vapply(ug, function(g) sum(gkey == g), integer(1))
        bad <- rep(FALSE, length(counts))
        if (!is.null(p$min)) bad <- bad | counts < p$min
        if (!is.null(p$max)) bad <- bad | counts > p$max
      } else {
        bad <- logical(0)
      }
      bad_rows <- if (any(bad)) unname(firsts[bad]) else integer(0)
      msg <- if (any(bad)) paste0("group count ", min(counts[bad]), " outside [",
        if (is.null(p$min)) "" else p$min, ", ",
        if (is.null(p$max)) "" else p$max, "]") else ""
      report(msg, bad_rows, gb_cols,
        list(counts = as.list(unname(counts[bad]))), length(counts))
    } else {
      bad <- FALSE
      if (!is.null(p$min) && n < p$min) bad <- TRUE
      if (!is.null(p$max) && n > p$max) bad <- TRUE
      bad_rows <- if (isTRUE(bad)) seq_len(n) else integer(0)
      msg <- if (isTRUE(bad)) paste0("row count ", n, " outside [",
        if (is.null(p$min)) "" else p$min, ", ",
        if (is.null(p$max)) "" else p$max, "]") else ""
      report(msg, bad_rows, ctx$keys, list(count = n), 1L)
    }
  } else if (kind == "implies") {
    w <- eval_pred(parse_predicate_text(p$`when`), resolver_of())
    t <- eval_pred(parse_predicate_text(p$then), resolver_of())
    bad <- !is.na(w) & w & (is.na(t) | !t)
    report(paste0(sum(bad), " rows violate the implication"), which(bad),
      ctx$keys, list(when = p$`when`, then = p$then), n)
  } else if (kind == "all_or_none") {
    cols <- p$columns
    bad <- vapply(seq_len(n), function(i) {
      miss <- vapply(cols, function(k) is.na(ctx$col[[k]]$v[i]), logical(1))
      any(miss) && !all(miss)
    }, logical(1))
    report(paste0(sum(bad), " rows partially missing"), which(bad), ctx$keys,
      list(columns = as.list(cols)), n)
  } else if (kind == "assert") {
    r <- eval_pred(parse_predicate_text(p$expr), resolver_of())
    bad <- is.na(r) | !r
    report(paste0(sum(bad), " rows fail the assertion"), which(bad), ctx$keys,
      list(expr = p$expr), n)
  } else {
    yamaa_error("invalid_spec", paste0("unknown dataset verification: ", kind))
  }
}

# does the spec declare any warning-severity verification? (REQ-0391)
spec_has_warning <- function(spec) {
  check <- function(vs) {
    for (v in normalize_verifications(vs)) {
      p <- v[[1]]
      if (!is.null(p$severity) && p$severity == "warning") return(TRUE)
    }
    FALSE
  }
  if (check(spec$verifications)) return(TRUE)
  for (c in spec$columns) if (check(c$verifications)) return(TRUE)
  FALSE
}

# ---- violation-log JSON (REQ-0394) ------------------------------------------
# compact ASCII JSON: no insignificant whitespace; object names in scalar
# order; short escapes where they exist, lower-case \u escapes otherwise;
# astral characters as surrogate pairs.

json_null <- function() structure(list(), class = "json_null")

json_escape <- function(s) {
  cps <- utf8ToInt(s)
  out <- character(length(cps))
  for (i in seq_along(cps)) {
    cp <- cps[i]
    out[i] <- if (cp == 0x22) "\\\""
    else if (cp == 0x5c) "\\\\"
    else if (cp == 0x08) "\\b"
    else if (cp == 0x0c) "\\f"
    else if (cp == 0x0a) "\\n"
    else if (cp == 0x0d) "\\r"
    else if (cp == 0x09) "\\t"
    else if (cp < 0x20 || cp > 0x7e) {
      if (cp <= 0xffff) sprintf("\\u%04x", cp)
      else {
        u <- cp - 0x10000
        sprintf("\\u%04x\\u%04x", 0xd800 + u %/% 0x400, 0xdc00 + u %% 0x400)
      }
    }
    else intToUtf8(cp)
  }
  paste0('"', paste(out, collapse = ""), '"')
}

json_write <- function(x) {
  if (inherits(x, "json_null")) return("null")
  if (is.null(x)) return("null")
  if (is.list(x)) {
    nm <- names(x)
    if (!is.null(nm) && any(nm != "")) {
      o <- order(vapply(nm, function(s)
        paste(sprintf("%06x", utf8ToInt(s)), collapse = "."), character(1)))
      parts <- vapply(o, function(i)
        paste0(json_escape(nm[i]), ":", json_write(x[[i]])), character(1))
      return(paste0("{", paste(parts, collapse = ","), "}"))
    }
    return(paste0("[", paste(vapply(x, json_write, character(1)),
      collapse = ","), "]"))
  }
  if (is.character(x)) {
    if (length(x) == 0) return("[]")
    if (length(x) > 1) return(json_write(as.list(x)))
    if (is.na(x)) return("null")
    return(json_escape(x))
  }
  if (is.logical(x)) {
    if (length(x) > 1) return(json_write(as.list(x)))
    if (is.na(x)) return("null")
    return(if (x) "true" else "false")
  }
  if (is.numeric(x)) {
    if (length(x) > 1) return(json_write(as.list(x)))
    if (is.na(x)) return("null")
    return(as.character(x))
  }
  json_escape(as.character(x))
}

# every offending row's key combination as canonical JSON (REQ-0393).
# Numbers use their str form; missing keys are null.
offending_keys_json <- function(ctx, rows, keynames) {
  if (length(rows) == 0 || length(keynames) == 0) return("[]")
  objs <- lapply(rows, function(i) {
    kv <- lapply(keynames, function(k) {
      c <- ctx$col[[k]]
      if (is.null(c)) return(json_null())
      v <- c$v[i]
      if (is.na(v)) return(json_null())
      str <- switch(c$t,
        int = as.character(v),
        float = float_text(v),
        date = v, datetime = v,
        as.character(convert_tv(tv(v, c$t), "str")$v))
      str
    })
    names(kv) <- keynames
    kv
  })
  json_write(objs)
}
