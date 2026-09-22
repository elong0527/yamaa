# temporal.R -- Temporal values (rules/values/temporal.md) and
# temporal operations (rules/operations/temporal.md).
#
# Dates are (y, m, d) integer triples; datetimes add (hh, mm, ss).
# Stored values are canonical text ("YYYY-MM-DD" / "YYYY-MM-DDThh:mm:ss")
# with type date/datetime, plus a parallel precision vector where an
# operation needs collected precision ("year"|"month"|"day").

# Howard Hinnant's days_from_civil: proleptic Gregorian -> serial day number.
days_from_civil <- function(y, m, d) {
  y <- y - (m <= 2); m <- m + 12 * (m <= 2)
  era <- floor((y - 1) / 400)
  yoe <- y - era * 400
  doy <- floor((153 * (m - 3) + 2) / 5) + d - 1
  doe <- yoe * 365 + floor(yoe / 4) - floor(yoe / 100) + doy
  era * 146097 + doe - 719468  # days since 1970-01-01
}

civil_from_days <- function(z) {
  z <- z + 719468
  era <- floor(z / 146097); doe <- z - era * 146097
  yoe <- floor((doe - floor(doe / 1460) + floor(doe / 36524) - floor(doe / 146096)) / 365)
  y <- yoe + era * 400
  doy <- doe - (365 * yoe + floor(yoe / 4) - floor(yoe / 100))
  mp <- floor((5 * doy + 2) / 153); d <- doy - floor((153 * mp + 2) / 5) + 1
  m <- mp + 3 - 12 * (mp >= 10); y <- y + (m <= 2)
  list(y = y, m = m, d = d)
}

month_length <- function(y, m) {
  if (m == 2) return(if ((y %% 4 == 0 & y %% 100 != 0) | y %% 400 == 0) 29L else 28L)
  c(31L,28L,31L,30L,31L,30L,31L,31L,30L,31L,30L,31L)[m]
}

# parse one text as date/datetime per the lexical grammar (REQ-0549..0553).
# returns list(ok, ymdhms[6], canon) or fails.
parse_temporal_one <- function(s, type) {
  bad <- function() yamaa_error("conversion_failed",
    paste0("not ", type, " text: ", s))
  if (type == "date") {
    m <- regmatches(s, regexec("^([0-9]{4})-([0-9]{2})-([0-9]{2})$", s))[[1]]
    if (length(m) == 0) bad()
    y <- as.integer(m[2]); mo <- as.integer(m[3]); d <- as.integer(m[4])
    h <- mi <- se <- 0L
  } else {
    m <- regmatches(s, regexec(
      "^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2})(?::([0-9]{2}))?$",
      s, perl = TRUE))[[1]]
    if (length(m) == 0) bad()
    y <- as.integer(m[2]); mo <- as.integer(m[3]); d <- as.integer(m[4])
    h <- as.integer(m[5]); mi <- as.integer(m[6])
    se <- if (nchar(m[7]) > 0) as.integer(m[7]) else 0L
  }
  if (y < 1 || y > 9999 || mo < 1 || mo > 12 || d < 1 || d > month_length(y, mo) ||
      h > 23 || mi > 59 || se > 59) bad()
  canon <- if (type == "date") sprintf("%04d-%02d-%02d", y, mo, d)
           else sprintf("%04d-%02d-%02dT%02d:%02d:%02d", y, mo, d, h, mi, se)
  list(y = y, mo = mo, d = d, h = h, mi = mi, se = se, canon = canon)
}

# vectorized: text -> canonical text of `type`; NA stays NA; bad text fails.
parse_temporal_text <- function(v, type) {
  vapply(v, function(s) {
    if (is.na(s)) NA_character_ else parse_temporal_one(s, type)$canon
  }, character(1), USE.NAMES = FALSE)
}

# split canonical text back into components (vectorized)
split_date <- function(v) {
  y <- as.integer(substr(v, 1, 4)); mo <- as.integer(substr(v, 6, 7)); d <- as.integer(substr(v, 9, 10))
  list(y = y, mo = mo, d = d)
}
split_datetime <- function(v) {
  c <- split_date(v)
  c$h <- as.integer(substr(v, 12, 13)); c$mi <- as.integer(substr(v, 15, 16))
  c$se <- as.integer(substr(v, 18, 19)); c
}

# ---- operations ---------------------------------------------------------
# date_diff(start, end, unit, bounds): int vectors, NA-aware (REQ-0594..0598)
op_date_diff <- function(start, end, unit = "day", bounds = "exclusive") {
  # bounds only applies to day counts; specifying it with other units is an error
  if (unit != "day" && bounds != "exclusive")
    yamaa_error("value_not_permitted", paste0("bounds not allowed with unit: ", unit))
  n <- length(start)
  out <- rep(NA_integer_, n)
  ok <- !is.na(start) & !is.na(end)
  if (!any(ok)) return(out)
  s <- split_date(start[ok]); e <- split_date(end[ok])
  ds <- days_from_civil(s$y, s$mo, s$d); de <- days_from_civil(e$y, e$mo, e$d)
  res <- switch(unit,
    day = de - ds,
    week = (de - ds) %/% 7L,   # truncates toward zero in R? %/% floors; fix below
    month = month_diff(s, e),
    year = year_diff(s, e),
    yamaa_error("value_not_permitted", paste0("unknown date_diff unit: ", unit)))
  if (unit == "week") {
    # whole seven-day blocks, remainder discarded: truncation toward zero
    res <- as.integer(trunc((de - ds) / 7))
  }
  if (unit == "day") {
    res <- as.integer(switch(bounds,
      exclusive = de - ds,
      inclusive = de - ds + 1L,
      between = de - ds - 1L,
      yamaa_error("value_not_permitted", paste0("unknown bounds: ", bounds))))
  }
  out[ok] <- res
  out
}

# count monthly anniversaries of start on/before end (REQ-0595)
month_diff <- function(s, e) {
  vapply(seq_along(s$y), function(i) {
    count_anniv(s$y[i], s$mo[i], s$d[i], e$y[i], e$mo[i], e$d[i], 1L)
  }, integer(1))
}
year_diff <- function(s, e) {
  vapply(seq_along(s$y), function(i) {
    count_anniv(s$y[i], s$mo[i], s$d[i], e$y[i], e$mo[i], e$d[i], 12L)
  }, integer(1))
}

# k-th anniversary of (y1,m1,d1) stepping `step` months; count those <= end.
# negative when end precedes start (REQ-0597).
count_anniv <- function(y1, m1, d1, y2, m2, d2, step) {
  ds <- days_from_civil(y1, m1, d1); de <- days_from_civil(y2, m2, d2)
  if (de < ds) return(-count_anniv(y2, m2, d2, y1, m1, d1, step))
  if (de == ds) return(0L)
  # upper bound on k: months between, +1
  k <- 0L
  total_months <- (y2 - y1) * 12L + (m2 - m1)
  # binary search would be fancier; linear from a close guess is fine
  k <- max(0L, total_months %/% step - 2L)
  anniv_day <- function(kk) {
    tm <- m1 + kk * step; yy <- y1 + (tm - 1L) %/% 12L; mm <- ((tm - 1L) %% 12L) + 1L
    dd <- min(d1, month_length(yy, mm))
    days_from_civil(yy, mm, dd)
  }
  while (anniv_day(k + 1L) <= de) k <- k + 1L
  while (k > 0L && anniv_day(k) > de) k <- k - 1L
  k
}

# study_day(date, reference): int, never zero (REQ-1108)
op_study_day <- function(date, reference) {
  n <- length(date); out <- rep(NA_integer_, n)
  ok <- !is.na(date) & !is.na(reference)
  if (!any(ok)) return(out)
  s <- split_date(date[ok]); r <- split_date(reference[ok])
  dd <- days_from_civil(s$y, s$mo, s$d) - days_from_civil(r$y, r$mo, r$d)
  out[ok] <- as.integer(ifelse(dd >= 0, dd + 1L, dd))
  out
}

# to_date(source): datetime -> date, or ISO date text -> date (REQ-0593/1107)
op_to_date <- function(src) {
  t <- src$t
  if (t == "datetime") {
    return(tv(ifelse(is.na(src$v), NA_character_, substr(src$v, 1, 10)), "date"))
  }
  if (t == "str") {
    return(tv(parse_temporal_text(src$v, "date"), "date"))
  }
  if (t == "date") yamaa_error("incompatible_input_type", "to_date does not accept a date")
  yamaa_error("incompatible_input_type", paste0("to_date source type: ", t))
}

# to_epoch_day(source): date -> int days since 1970-01-01 (REQ-1187/1188).
op_to_epoch_day <- function(src) {
  t <- src$t
  if (t != "date")
    yamaa_error("incompatible_input_type",
      paste0("to_epoch_day needs a date source, got ", t))
  v <- src$v
  out <- rep(NA_integer_, length(v))
  ok <- !is.na(v)
  if (any(ok)) {
    s <- split_date(v[ok])
    out[ok] <- as.integer(days_from_civil(s$y, s$mo, s$d))
  }
  tv(out, "int")
}

# date_precision(source): str text or date value -> "Y"/"M"/"D" (REQ-0580/1106)
# REQ-0588: non-missing text that is neither a complete date nor a date
op_date_precision <- function(src, missing_h, invalid_h) {
  if (src$t == "date") {
    prec <- attr(src, "precision")
    if (is.null(prec)) prec <- rep("day", length(src$v))
    code <- ifelse(is.na(src$v), NA_character_,
      ifelse(prec == "year", "Y", ifelse(prec == "month", "M", "D")))
    return(tv(code, "str"))
  }
  if (src$t != "str") yamaa_error("incompatible_input_type", "date_precision source must be str or date")
  vapply(src$v, function(s) {
    if (is.na(s)) return(missing_h)          # handler literal or NA
    if (grepl("^[0-9]{4}$", s)) return("Y")
    if (grepl("^[0-9]{4}-[0-9]{2}$", s)) {
      mo <- as.integer(substr(s, 6, 7))
      if (mo >= 1 && mo <= 12) return("M")
      if (is.null(invalid_h))
        yamaa_error("invalid_date_text", paste0("date_precision: not a date: ", s))
      return(invalid_h)
    }
    if (!is.null(try_catch_null(parse_temporal_one(s, "date")))) return("D")
    if (is.null(invalid_h))
      yamaa_error("invalid_date_text", paste0("date_precision: not a date: ", s))
    invalid_h
  }, character(1), USE.NAMES = FALSE) |> tv("str")
}

try_catch_null <- function(expr) tryCatch(expr, error = function(e) NULL)

# date_impute(source, month, day, minimum_source_precision, not_before,
#             missing_h, invalid_h, ctx): -> date tv (REQ-0583..0588, REQ-1105)
# invalid_h "ABSENT" = key absent -> fail invalid_date_text (REQ-0588);
op_date_impute <- function(src, month, day, min_prec = "year", not_before = NULL,
                           missing_h = NA_character_, invalid_h = "ABSENT", ctx = NULL) {
  if (src$t != "str") yamaa_error("incompatible_input_type", "date_impute source must be str")
  n <- length(src$v)
  out <- rep(NA_character_, n); prec <- rep(NA_character_, n)
  for (i in seq_len(n)) {
    s <- src$v[i]
    if (is.na(s)) { out[i] <- missing_h; next }
    kind <- classify_partial(s)  # "day"|"month"|"year"|"invalid"
    if (kind == "invalid") {
      if (identical(invalid_h, "ABSENT"))
        yamaa_error("invalid_date_text", paste0("not a date: ", s))
      out[i] <- invalid_h; next
    }
    if (kind == "year" && min_prec == "month") next  # stays missing, no handler
    y <- as.integer(substr(s, 1, 4))
    if (kind == "day") {
      p <- parse_temporal_one(s, "date")  # validates calendar
      out[i] <- p$canon; prec[i] <- "day"; next
    }
    mo <- if (kind == "month") as.integer(substr(s, 6, 7))
          else resolve_month(month, i)
    dd <- resolve_day(day, i, y, mo)
    if (is.null(mo) || is.null(dd)) next  # shouldn't happen; validated earlier
    if (kind == "year") prec[i] <- "year" else prec[i] <- "month"
    cand <- sprintf("%04d-%02d-%02d", y, mo, dd)
    # REQ-0608: the completed date must be a valid calendar date
    if (dd > month_length(y, mo))
      yamaa_error("invalid_calendar_date", paste0("invalid date: ", cand))
    # not_before moves only imputed components, within the admitted interval
    if (!is.null(not_before) && !is.na(not_before[i])) {
      nb <- not_before[i]
      lo <- if (kind == "year") sprintf("%04d-01-01", y) else sprintf("%04d-%02d-01", y, mo)
      hi <- if (kind == "year") sprintf("%04d-12-31", y)
            else sprintf("%04d-%02d-%02d", y, mo, month_length(y, mo))
      if (cand < nb) {
        cand <- if (nb <= hi) nb else NA_character_  # earliest admissible day >= bound
      }
      if (is.na(cand) || cand < lo) { out[i] <- NA_character_; prec[i] <- NA_character_; next }
    }
    out[i] <- cand
  }
  r <- tv(out, "date"); attr(r, "precision") <- prec; r
}

classify_partial <- function(s) {
  if (grepl("^[0-9]{4}$", s)) return("year")
  if (grepl("^[0-9]{4}-[0-9]{2}$", s)) {
    mo <- as.integer(substr(s, 6, 7))
    if (mo >= 1 && mo <= 12) return("month")
    return("invalid")
  }
  if (!is.null(try_catch_null(parse_temporal_one(s, "date")))) return("day")
  "invalid"
}

# datetime_impute(source, time, missing_h, invalid_h): -> datetime tv (REQ-1182)
op_datetime_impute <- function(src, time, missing_h = NA_character_, invalid_h = "ABSENT") {
  if (is.null(time) || !time %in% c("first", "last"))
    yamaa_error("value_not_permitted",
      paste0("datetime_impute time must be first or last: ", time))
  if (src$t == "datetime") {
    r <- tv(src$v, "datetime")
    attr(r, "precision") <- ifelse(is.na(src$v), NA_character_, "second")
    return(r)
  }
  if (src$t == "date") {
    r <- tv(ifelse(is.na(src$v), NA_character_,
      paste0(src$v, if (time == "first") "T00:00:00" else "T23:59:59")), "datetime")
    attr(r, "precision") <- ifelse(is.na(src$v), NA_character_, "day")
    return(r)
  }
  if (src$t != "str")
    yamaa_error("incompatible_input_type", "datetime_impute source must be str, date, or datetime")
  n <- length(src$v)
  out <- rep(NA_character_, n); prec <- rep(NA_character_, n)
  edge <- if (time == "first") "T00:00:00" else "T23:59:59"
  for (i in seq_len(n)) {
    s <- src$v[i]
    if (is.na(s)) { out[i] <- missing_h; next }
    if (!is.null(try_catch_null(parse_temporal_one(s, "datetime")))) {
      out[i] <- parse_temporal_one(s, "datetime")$canon; prec[i] <- "second"; next
    }
    if (!is.null(try_catch_null(parse_temporal_one(s, "date")))) {
      out[i] <- paste0(parse_temporal_one(s, "date")$canon, edge); prec[i] <- "day"; next
    }
    if (identical(invalid_h, "ABSENT"))
      yamaa_error("invalid_datetime_text", paste0("not a datetime: ", s))
    out[i] <- invalid_h
  }
  r <- tv(out, "datetime"); attr(r, "precision") <- prec; r
}

# datetime_precision(source): str text or datetime value -> "S"/"D" (REQ-1183)
op_datetime_precision <- function(src, missing_h, invalid_h) {
  if (src$t == "datetime") {
    prec <- attr(src, "precision")
    if (is.null(prec)) prec <- rep("second", length(src$v))
    code <- ifelse(is.na(src$v), NA_character_,
      ifelse(prec == "second", "S", "D"))
    return(tv(code, "str"))
  }
  if (src$t != "str")
    yamaa_error("incompatible_input_type", "datetime_precision source must be str or datetime")
  vapply(src$v, function(s) {
    if (is.na(s)) return(missing_h)
    if (!is.null(try_catch_null(parse_temporal_one(s, "datetime")))) return("S")
    if (!is.null(try_catch_null(parse_temporal_one(s, "date")))) return("D")
    if (is.null(invalid_h))
      yamaa_error("invalid_datetime_text", paste0("datetime_precision: not a datetime: ", s))
    invalid_h
  }, character(1), USE.NAMES = FALSE) |> tv("str")
}

resolve_month <- function(month, i) {
  m <- month[[i]]  # literal int per spec (scalar or per-row)
  if (is.na(m) || m < 1 || m > 12) yamaa_error("invalid_argument", "date_impute month out of range")
  as.integer(m)
}

resolve_day <- function(day, i, y, mo) {
  d <- day[[i]]
  if (is.character(d)) {
    if (d == "first") return(1L)
    if (d == "last") return(month_length(y, mo))
    yamaa_error("value_not_permitted", paste0("date_impute day token: ", d))
  }
  if (is.na(d) || d < 1 || d > 31) yamaa_error("value_not_permitted", "date_impute day out of range")
  as.integer(d)
}
