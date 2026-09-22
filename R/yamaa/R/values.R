# YAML 1.1 'Y'/'y'/'N'/'n' are strings, not booleans (REQ-0714 needs
# distinct Y/y dict keys). Other booleans (true/false) stay logical.
yaml_handlers <- list(
  "bool#yes" = function(x) if (x %in% c("Y", "y")) x else TRUE,
  "bool#no" = function(x) if (x %in% c("N", "n")) x else FALSE)

yaml_load_file <- function(path) yaml::yaml.load_file(path, handlers = yaml_handlers)

# values.R -- the yamaa type system in base R.
#
# A "typed vector" (tv) is list(v = <R vector>, t = <type>), type one of
# "str" | "int" | "float" | "date" | "datetime".
#   str      -> character
#   int      -> integer (32-bit R integer; values outside that range fail loudly)
#   float    -> double
#   date/datetime -> character holding canonical fixed-width text, so
#                   chronological order == string order (REQ-0003).
# Missing is NA of the matching storage type. Non-finite doubles become
# missing at every boundary (REQ-0006).

yamaa_error <- function(condition, message) {
  e <- simpleError(paste0("[", condition, "] ", message))
  class(e) <- c("yamaa_error", class(e))
  attr(e, "yamaa_condition") <- condition
  stop(e)
}

YAMAA_TYPES <- c("str", "int", "float", "date", "datetime", "null", "bool")

# declared (public) column types: "null" is internal only and can never be
# a declared column type.
PUBLIC_TYPES <- c("str", "int", "float", "date", "datetime")

validate_column_types <- function(spec) {
  for (c in spec$columns) {
    if (is.null(c$type)) {
      # REQ-0660: an invalid null clearing marker fails with invalid_clear
      yamaa_error("invalid_clear",
        paste0("column ", c$name, " has invalid null type marker"))
    }
    if (!c$type %in% PUBLIC_TYPES)
      yamaa_error("value_not_permitted",
        paste0("column ", c$name, " has invalid declared type: ", c$type))
  }
  invisible(NULL)
}

tv <- function(v, t) {
  if (!t %in% YAMAA_TYPES) yamaa_error("unknown_type", paste0("not a column type: ", t))
  list(v = v, t = t)
}

tv_len <- function(x) length(x$v)

# storage constructors -------------------------------------------------
tv_na <- function(t, n) {
  v <- switch(t,
    str = rep(NA_character_, n),
    int = rep(NA_integer_, n),
    float = rep(NA_real_, n),
    date = rep(NA_character_, n),
    datetime = rep(NA_character_, n),
    bool = rep(NA, n),
    null = rep(NA, n))
  tv(v, t)
}

# numeric text parsing (REQ-0015) ---------------------------------------
parse_number_text <- function(s) {
  vapply(s, function(one) {
    if (is.na(one)) return(NA_real_)
    if (grepl("^[+-]?[.]inf$", one, ignore.case = TRUE)) return(NA_real_)  # normalized
    if (grepl("^[.]nan$", one, ignore.case = TRUE)) return(NA_real_)       # normalized
    if (!grepl("^[+-]?([0-9]+(\\.[0-9]*)?|\\.[0-9]+)([eE][+-]?[0-9]+)?$", one)) {
      yamaa_error("conversion_failed", paste0("not numeric text: ", one))
    }
    suppressWarnings(as.numeric(one))
  }, double(1), USE.NAMES = FALSE)
}

# R's yaml parses bare Y/N as booleans (YAML 1.1); in string contexts they
# mean the strings "Y"/"N". Convert back when a string is expected.
y_string <- function(x) {
  if (is.logical(x) && length(x) == 1 && !is.na(x))
    return(if (x) "Y" else "N")
  as.character(x)
}

# REQ-0010: the conversion matrix ----------------------------------------
apply_declared_type <- function(tv_in, declared, cf_lit, cf_present, colname) {
  if (tv_in$t == declared) return(tv_in)
  if (!cf_present) {
    # fail loudly on the first bad value
    return(convert_tv(tv_in, declared))
  }
  n <- tv_len(tv_in)
  out <- tv_na(declared, n)
  fb <- if (is.null(cf_lit)) tv_na(declared, 1L)$v else literal_to_tv(cf_lit, declared, 1L)$v
  for (i in seq_len(n)) {
    one <- convert_tv(tv(tv_in$v[i], tv_in$t), declared, on_fail = "na")
    out$v[i] <- if (is.na(one$v)) fb else one$v
  }
  out
}

# double -> 32-bit R integer; NA stays NA; fractional values fail as
# conversion_failed; whole values outside the 32-bit range fail as
# integer_overflow (REQ-0434). Backs str->int / float->int and readers.
to_int_value <- function(x) {
  if (is.na(x)) return(NA_integer_)
  if (x != trunc(x))
    yamaa_error("conversion_failed", paste0("not an integer value: ", x))
  if (x < -2147483648 || x > 2147483647)
    yamaa_error("integer_overflow", paste0("integer overflow: ", x))
  as.integer(x)
}

# REQ-0006: non-finite doubles become missing at every boundary
norm_finite <- function(x) {
  if (x$t == "float") {
    v <- x$v; v[!is.finite(v)] <- NA_real_; x$v <- v
  }
  x
}

# convert_tv with on_fail="na": per-element NA instead of raising
convert_tv <- function(x, dest, on_fail = "raise") {
  if (on_fail == "na") {
    return(tryCatch(convert_tv(x, dest),
      yamaa_error = function(e) tv_na(dest, tv_len(x))))
  }
  x <- norm_finite(x)
  if (x$t == dest) return(x)
  # internal null (e.g. `literal: null` in a case branch) converts to any
  # declared type as all-missing; null is never a declared column type.
  if (x$t == "null") return(tv_na(dest, tv_len(x)))
  v <- x$v
  out <- switch(paste0(x$t, "->", dest),
    "str->int" = vapply(parse_number_text(v), to_int_value, integer(1)),
    "str->float" = parse_number_text(v),
    "str->date" = parse_temporal_text(v, "date"),
    "str->datetime" = parse_temporal_text(v, "datetime"),
    "int->str" = ifelse(is.na(v), NA_character_, as.character(v)),
    "int->float" = as.numeric(v),
    "float->str" = float_text(v),
    "float->int" = vapply(v, to_int_value, integer(1)),
    "date->str" = v,
    "datetime->str" = v,
    "bool->str" = ifelse(is.na(v), NA_character_, ifelse(v, "Y", "N")),
    yamaa_error("conversion_failed",
      paste0("no conversion from ", x$t, " to ", dest)))
  tv(out, dest)
}

# REQ-0018: shortest round-trip digits in positional notation, no exponent.
float_text <- function(v) {
  vapply(v, function(one) {
    if (is.na(one)) return(NA_character_)
    s <- shortest_float_text(one)
    expand_sci(s)
  }, character(1), USE.NAMES = FALSE)
}

# shortest decimal text that parses back to the same binary64
shortest_float_text <- function(x) {
  for (k in 1:17) {
    s <- sprintf(paste0("%.", k, "g"), x)
    if (suppressWarnings(as.numeric(s)) == x) return(s)
  }
  sprintf("%.17g", x)
}

# expand "d[.ddd]e+/-XX" to positional notation (exact: digits are untouched)
expand_sci <- function(s) {
  if (!grepl("[eE]", s)) return(s)
  m <- regmatches(s, regexec("^([+-]?)([0-9]+)(?:\\.([0-9]*))?[eE]([+-]?[0-9]+)$", s))[[1]]
  if (length(m) == 0) return(s)
  sign <- m[2]; intp <- m[3]; fracp <- if (nchar(m[4]) > 0) m[4] else ""; e <- as.integer(m[5])
  digits <- paste0(intp, fracp)
  point <- nchar(intp) + e   # digits left of the point
  out <- if (point <= 0) {
    paste0("0.", strrep("0", -point), digits)
  } else if (point >= nchar(digits)) {
    paste0(digits, strrep("0", point - nchar(digits)))
  } else {
    paste0(substr(digits, 1, point), ".", substr(digits, point + 1, nchar(digits)))
  }
  # strip trailing zeros in fraction, and a dangling point
  if (grepl("\\.", out)) {
    out <- sub("0+$", "", out); out <- sub("\\.$", "", out)
  }
  if (out == "" || out == "-") out <- "0"
  paste0(sign, out)
}

# typed comparison for predicates / MIN / MAX / ordering ----------------
# returns -1/0/1/NA per element pair; fails on incomparable types (REQ-0323).
cmp_typed <- function(a, b) {
  ta <- a$t; tb <- b$t
  if (ta != tb && !((ta %in% c("int", "float")) && (tb %in% c("int", "float")))) {
    yamaa_error("incompatible_input_type",
      paste0("cannot compare ", ta, " with ", tb))
  }
  av <- a$v; bv <- b$v
  num <- ta %in% c("int", "float") || tb %in% c("int", "float")
  if (num) { av <- as.numeric(av); bv <- as.numeric(bv) }
  # REQ-0026: character comparison is Unicode scalar-value order. R's own
  # string comparison is locale-dependent (and yields NA for non-ASCII in a
  # C locale), so compare code points directly.
  res <- rep(NA_real_, length(av))
  ok <- !is.na(av) & !is.na(bv)
  if (ta == "str" && tb == "str") {
    oi <- which(ok)
    res[oi] <- vapply(oi, function(i) cmp_str_scalar(av[i], bv[i]), numeric(1))
  } else {
    res[ok] <- sign((av[ok] > bv[ok]) - (av[ok] < bv[ok]))
  }
  res
}

# Unicode scalar-value comparison of two strings: -1/0/1.
cmp_str_scalar <- function(a, b) {
  ai <- utf8ToInt(a); bi <- utf8ToInt(b)
  n <- min(length(ai), length(bi))
  if (n > 0) {
    d <- ai[seq_len(n)] - bi[seq_len(n)]
    nz <- which(d != 0L)
    if (length(nz) > 0) return(sign(d[nz[1L]]))
  }
  sign(length(ai) - length(bi))
}

# three-valued logic helpers (REQ-0171 tables) -------------------------------
tv_and <- function(a, b) {
  af <- !is.na(a) & !a; bf <- !is.na(b) & !b
  at <- !is.na(a) & a;  bt <- !is.na(b) & b
  ifelse(af | bf, FALSE, ifelse(at & bt, TRUE, NA))
}
tv_or <- function(a, b) {
  at <- !is.na(a) & a;  bt <- !is.na(b) & b
  af <- !is.na(a) & !a; bf <- !is.na(b) & !b
  ifelse(at | bt, TRUE, ifelse(af & bf, FALSE, NA))
}
tv_not <- function(a) {
  ifelse(is.na(a), NA, !a)
}
