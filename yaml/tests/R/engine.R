# Reference derivation engine, R implementation.
#
# The counterpart of tests/python/{predicates,engine,functions}.py. It must
# produce byte-identical serialized output to Python for the same specification
# and inputs; run_parity.R checks that.

# Host functions live in functions.R so validate.R can check `call` without
# pulling in the whole engine.
source(file.path(Sys.getenv("YAMAA_YAML_DIR"), "tests", "R", "functions.R"))

# R004 predicates -------------------------------------------------------------

KEYWORDS <- c("AND", "OR", "NOT", "IS", "NULL", "IN", "TRUE", "FALSE")

tokenize <- function(text) {
  pat <- paste0("^\\s*(?:('(?:[^']|'')*')",      # 1 string
                "|(-?[0-9]+(?:\\.[0-9]+)?)",      # 2 number
                "|(<>|<=|>=|=|<|>)",              # 3 operator
                "|(\\()|(\\))|(,)",               # 4 5 6
                "|([A-Za-z_][A-Za-z0-9_.]*))")    # 7 identifier
  out <- list(); rest <- text
  while (nzchar(trimws(rest))) {
    m <- regmatches(rest, regexec(pat, rest, perl = TRUE))[[1]]
    if (!length(m)) stop(sprintf("cannot tokenize predicate at '%s'", rest))
    rest <- substring(rest, nchar(m[1]) + 1L)
    g <- m[-1]
    if (nzchar(g[1])) {
      lit <- gsub("''", "'", substr(g[1], 2, nchar(g[1]) - 1L))
      out[[length(out) + 1L]] <- list(kind = "lit", value = lit)
    } else if (nzchar(g[2])) {
      out[[length(out) + 1L]] <- list(kind = "lit", value = as.numeric(g[2]))
    } else if (nzchar(g[3])) {
      out[[length(out) + 1L]] <- list(kind = "op", value = g[3])
    } else if (nzchar(g[4])) {
      out[[length(out) + 1L]] <- list(kind = "lparen", value = "(")
    } else if (nzchar(g[5])) {
      out[[length(out) + 1L]] <- list(kind = "rparen", value = ")")
    } else if (nzchar(g[6])) {
      out[[length(out) + 1L]] <- list(kind = "comma", value = ",")
    } else {
      v <- g[7]
      if (toupper(v) %in% KEYWORDS) {
        out[[length(out) + 1L]] <- list(kind = "kw", value = toupper(v))
      } else {
        out[[length(out) + 1L]] <- list(kind = "ident", value = v)
      }
    }
  }
  out
}

predicate_vars <- function(text) {
  toks <- tokenize(text)
  unlist(lapply(toks, function(t) if (identical(t$kind, "ident")) t$value))
}

is_missing <- function(v) is.null(v) || (length(v) == 1L && (is.na(v) || identical(v, "")))

coerce_pair <- function(a, b) {
  na <- suppressWarnings(as.numeric(a)); nb <- suppressWarnings(as.numeric(b))
  if (!is.na(na) && !is.na(nb)) list(na, nb) else list(as.character(a), as.character(b))
}

compare_op <- function(op, a, b) {
  p <- coerce_pair(a, b); x <- p[[1]]; y <- p[[2]]
  switch(op, "=" = x == y, "<>" = x != y, "<" = x < y,
         "<=" = x <= y, ">" = x > y, ">=" = x >= y, FALSE)
}

# Three-valued logic: NA means UNKNOWN.
evaluate_predicate <- function(text, lookup) {
  toks <- tokenize(text); i <- 1L
  peek <- function() if (i <= length(toks)) toks[[i]] else list(kind = NA, value = NA)
  eat <- function() { t <- peek(); i <<- i + 1L; t }

  operand <- function() {
    t <- peek()
    if (identical(t$kind, "lit")) { eat(); return(t$value) }
    if (identical(t$kind, "ident")) { eat(); return(lookup(t$value)) }
    if (identical(t$kind, "kw") && t$value %in% c("TRUE", "FALSE")) {
      eat(); return(identical(t$value, "TRUE"))
    }
    stop(sprintf("expected an operand, got '%s'", t$value))
  }

  comparison <- function() {
    if (identical(peek()$kind, "lparen")) {
      eat(); v <- disjunction()
      if (!identical(peek()$kind, "rparen")) stop("expected rparen")
      eat(); return(v)
    }
    left <- operand(); t <- peek()
    if (identical(t$kind, "kw") && identical(t$value, "IS")) {
      eat(); neg <- FALSE
      if (identical(peek()$value, "NOT")) { eat(); neg <- TRUE }
      if (!identical(peek()$value, "NULL")) stop("expected NULL")
      eat(); miss <- is_missing(left)
      return(if (neg) !miss else miss)
    }
    if (identical(t$kind, "kw") && identical(t$value, "IN")) {
      eat()
      if (!identical(peek()$kind, "lparen")) stop("expected lparen")
      eat(); members <- list(operand())
      while (identical(peek()$kind, "comma")) { eat(); members[[length(members) + 1L]] <- operand() }
      if (!identical(peek()$kind, "rparen")) stop("expected rparen")
      eat()
      if (is_missing(left)) return(NA)
      return(any(vapply(members, function(m) isTRUE(compare_op("=", left, m)), logical(1))))
    }
    if (identical(t$kind, "op")) {
      eat(); right <- operand()
      if (is_missing(left) || is_missing(right)) return(NA)
      return(compare_op(t$value, left, right))
    }
    stop(sprintf("expected a comparison operator, got '%s'", t$value))
  }

  negation <- function() {
    if (identical(peek()$kind, "kw") && identical(peek()$value, "NOT")) {
      eat(); v <- negation(); return(if (is.na(v)) NA else !v)
    }
    comparison()
  }
  conjunction <- function() {
    v <- negation()
    while (identical(peek()$kind, "kw") && identical(peek()$value, "AND")) {
      eat(); r <- negation()
      v <- if (isFALSE(v) || isFALSE(r)) FALSE else if (is.na(v) || is.na(r)) NA else TRUE
    }
    v
  }
  disjunction <- function() {
    v <- conjunction()
    while (identical(peek()$kind, "kw") && identical(peek()$value, "OR")) {
      eat(); r <- conjunction()
      v <- if (isTRUE(v) || isTRUE(r)) TRUE else if (is.na(v) || is.na(r)) NA else FALSE
    }
    v
  }
  v <- disjunction()
  if (i != length(toks) + 1L) stop("trailing tokens in predicate")
  isTRUE(v)
}

# R005 conversion and serialization -------------------------------------------

convert_value <- function(value, declared) {
  if (is_missing(value)) return(NULL)
  if (identical(declared, "str")) return(as.character(value))
  if (identical(declared, "int")) {
    if (is.logical(value)) stop("bool is not an int (R006)")
    n <- suppressWarnings(as.numeric(value))
    if (is.na(n)) stop(sprintf("cannot convert '%s' to int", value))
    if (n != round(n)) stop(sprintf("cannot convert '%s' to int without loss", value))
    return(as.integer(round(n)))
  }
  if (identical(declared, "float")) {
    n <- suppressWarnings(as.numeric(value))
    if (is.na(n)) stop(sprintf("cannot convert '%s' to float", value))
    return(n)
  }
  if (identical(declared, "date")) {
    s <- as.character(value)
    if (!grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}$", s)) {
      stop(sprintf("'%s' is not an ISO 8601 complete date", s))
    }
    return(s)
  }
  stop(sprintf("unknown column type '%s'; R005 leaves the type vocabulary open", declared))
}

# Must match tests/python/engine.py serialize() exactly.
serialize_value <- function(value) {
  if (is_missing(value)) return("")
  if (is.logical(value)) return(if (isTRUE(value)) "TRUE" else "FALSE")
  if (is.numeric(value)) {
    if (value == round(value)) return(format(as.integer(round(value)), scientific = FALSE))
    s <- format(round(value, 10), scientific = FALSE, digits = 15)
    # Strip trailing zeros only inside a decimal fraction. Stripping
    # unconditionally turns "50" into "5".
    if (grepl(".", s, fixed = TRUE)) {
      s <- sub("0+$", "", s); s <- sub("\\.$", "", s)
    }
    return(s)
  }
  as.character(value)
}

# Operations ------------------------------------------------------------------

fold_ascii_chr <- function(s) {
  ch <- strsplit(as.character(s), "")[[1]]
  paste(ifelse(ch >= "A" & ch <= "Z", tolower(ch), ch), collapse = "")
}

UNMAPPED <- "yamaa_unmapped"

op_mapping <- function(value, args, ctx) {
  tbl <- args$dict
  cs <- if (is.null(args$case_sensitive)) TRUE else isTRUE(args$case_sensitive)
  if (cs) {
    if (!is_missing(value) && as.character(value) %in% names(tbl)) return(tbl[[as.character(value)]])
  } else {
    folded <- stats::setNames(tbl, vapply(names(tbl), fold_ascii_chr, ""))
    key <- if (is_missing(value)) "" else fold_ascii_chr(value)
    if (key %in% names(folded)) return(folded[[key]])
  }
  stop(UNMAPPED)
}

op_mapping_from <- function(value, args, ctx) {
  tbl <- ctx$data[[args$dataset]]
  hits <- tbl[vapply(tbl, function(r) identical(as.character(r[[args$key]]), as.character(value)),
                     logical(1))]
  if (length(hits) > 1L) stop(sprintf("mapping_from: '%s' is not unique on %s", args$dataset, args$key))
  if (!length(hits)) stop(UNMAPPED)
  hits[[1]][[args$value]]
}

op_multiply <- function(value, args, ctx) as.numeric(value) * as.numeric(args$factor)
op_add      <- function(value, args, ctx) as.numeric(value) + as.numeric(args$addend)

op_subtract <- function(value, args, ctx) {
  if (is_missing(args$minuend) || is_missing(args$subtrahend)) return(NULL)
  as.numeric(args$minuend) - as.numeric(args$subtrahend)
}

op_percent_change <- function(value, args, ctx) {
  if (is_missing(args$base) || is_missing(args$value)) return(NULL)
  b <- as.numeric(args$base)
  if (b == 0) return(NULL)
  100 * (as.numeric(args$value) - b) / b
}

op_coalesce <- function(value, args, ctx) {
  for (v in args$values) if (!is_missing(v)) return(v)
  NULL
}

op_cut <- function(value, args, ctx) {
  if (is_missing(value)) stop(UNMAPPED)
  breaks <- unlist(args$breaks); labels <- unlist(args$labels)
  if (length(labels) != length(breaks) + 1L) stop("cut: labels must have len(breaks) + 1 entries")
  right <- isTRUE(args$right)
  x <- as.numeric(value)
  for (i in seq_along(breaks)) {
    if (if (right) x <= breaks[i] else x < breaks[i]) return(labels[i])
  }
  labels[length(labels)]
}

op_str_extract <- function(value, args, ctx) {
  m <- regmatches(as.character(value), regexec(args$pattern, as.character(value), perl = TRUE))[[1]]
  if (!length(m)) stop(UNMAPPED)
  grp <- if (is.null(args$group)) 0L else as.integer(args$group)
  m[grp + 1L]
}

op_date_diff <- function(value, args, ctx) {
  if (is_missing(args$start) || is_missing(args$end)) return(NULL)
  a <- as.Date(substr(as.character(args$start), 1, 10))
  b <- as.Date(substr(as.character(args$end), 1, 10))
  unit <- args$unit
  if (identical(unit, "day")) return(as.integer(b - a))
  if (identical(unit, "week")) return(as.integer(b - a) %/% 7L)
  months <- (as.integer(format(b, "%Y")) - as.integer(format(a, "%Y"))) * 12L +
    (as.integer(format(b, "%m")) - as.integer(format(a, "%m"))) -
    (if (as.integer(format(b, "%d")) < as.integer(format(a, "%d"))) 1L else 0L)
  if (identical(unit, "month")) months else months %/% 12L
}

op_case <- function(value, args, ctx) {
  for (br in args$branches) {
    if (evaluate_predicate(br$when, ctx$row_lookup)) return(br$then)
  }
  args[["else"]]
}

op_call <- function(value, args, ctx) {
  fn <- HOST_FUNCTIONS[[args$function_name]]
  if (is.null(fn)) {
    stop(sprintf("call: '%s' is not in the host function library (available: %s)",
                 args$function_name, paste(sort(names(HOST_FUNCTIONS)), collapse = ", ")))
  }
  do.call(fn, args$args %||% list())
}

SCALAR_OPS <- list(
  mapping = op_mapping, mapping_from = op_mapping_from, multiply = op_multiply,
  add = op_add, subtract = op_subtract, percent_change = op_percent_change,
  coalesce = op_coalesce, cut = op_cut, str_extract = op_str_extract,
  date_diff = op_date_diff, case = op_case, call = op_call
)

ord_key <- function(v) {
  if (is_missing(v)) return(c(2, 0))
  n <- suppressWarnings(as.numeric(v))
  if (!is.na(n)) c(0, n) else c(1, 0)
}

sort_index <- function(values) {
  cls <- vapply(values, function(v) ord_key(v)[1], 0)
  num <- vapply(values, function(v) ord_key(v)[2], 0)
  chr <- vapply(values, function(v) if (is_missing(v)) "" else as.character(v), "")
  order(cls, num, chr, seq_along(values))
}

win_row_number <- function(rows, args, ctx) {
  keys <- args$order_by_per_row
  n <- length(rows)
  ranked <- do.call(order, c(
    lapply(seq_along(keys[[1]]), function(j) {
      vals <- lapply(keys, function(k) k[[j]])
      cls <- vapply(vals, function(v) ord_key(v)[1], 0)
      num <- vapply(vals, function(v) ord_key(v)[2], 0)
      chr <- vapply(vals, function(v) if (is_missing(v)) "" else as.character(v), "")
      list(cls, num, chr)
    }) |> unlist(recursive = FALSE),
    list(seq_len(n))
  ))
  out <- integer(n); out[ranked] <- seq_len(n); as.list(out)
}

win_baseline_flag <- function(rows, args, ctx) {
  dates <- args$date_per_row; refs <- args$reference_date_per_row
  best <- NULL; best_i <- NULL
  for (i in seq_along(dates)) {
    if (is_missing(dates[[i]]) || is_missing(refs[[i]])) next
    d <- as.character(dates[[i]])
    if (d <= as.character(refs[[i]]) && (is.null(best) || d > best)) { best <- d; best_i <- i }
    else if (!is.null(best) && d == best && !identical(i, best_i)) {
      stop("baseline_flag: tie for the baseline record")
    }
  }
  out <- vector("list", length(rows))
  if (!is.null(best_i)) out[best_i] <- list("Y")
  out
}

win_baseline_value <- function(rows, args, ctx) {
  vals <- args$value_per_row; flags <- args$flag_per_row
  picked <- NULL
  for (i in seq_along(flags)) if (identical(flags[[i]], "Y")) { picked <- vals[[i]]; break }
  rep(list(picked), length(rows))
}

WINDOW_OPS <- list(row_number = win_row_number, baseline_flag = win_baseline_flag,
                   baseline_value = win_baseline_value)

AGGREGATE_OPS <- list(
  min = function(vals) {
    keep <- Filter(function(v) !is_missing(v), vals)
    if (!length(keep)) return(NULL)
    keep[[sort_index(keep)[1]]]
  },
  max = function(vals) {
    keep <- Filter(function(v) !is_missing(v), vals)
    if (!length(keep)) return(NULL)
    keep[[sort_index(keep)[length(keep)]]]
  }
)
