# grammar.R -- tokenizers and recursive-descent parsers for the three closed
# grammars: predicate (R004), numeric/compute (R010), aggregate (R013).
# ASTs are tagged lists: list(kind=..., ...).
#
# A keyword is case-insensitive and cannot be a bare identifier; a qualified
# field may still use a reserved spelling after its qualifier (REQ-0039).
# Identifier spelling itself is case-sensitive (REQ-0040).

PRED_RESERVED <- c("AND","BETWEEN","DATE","DATETIME","ESCAPE","FALSE","IN",
  "IS","LIKE","NOT","NULL","OR","TRUE","UNKNOWN")

tokenize <- function(text) {
  toks <- list(); i <- 1L; n <- nchar(text)
  fail <- function(msg) yamaa_error("parse_failure",
    paste0(msg, " in: ", text))
  while (i <= n) {
    ch <- substr(text, i, i)
    if (ch %in% c(" ", "\t", "\n", "\r")) { i <- i + 1L; next }
    # string literal
    if (ch == "'") {
      j <- i + 1L; buf <- ""
      closed <- FALSE
      while (j <= n) {
        c2 <- substr(text, j, j)
        if (c2 == "'") {
          if (j + 1L <= n && substr(text, j + 1L, j + 1L) == "'") {
            buf <- paste0(buf, "'"); j <- j + 2L; next
          }
          closed <- TRUE; j <- j + 1L; break
        }
        buf <- paste0(buf, c2); j <- j + 1L
      }
      if (!closed) fail("unterminated string literal")
      toks[[length(toks) + 1]] <- list(kind = "string", text = buf); i <- j; next
    }
    # number (sign handled by parser context); a lone '.' is a qualifier dot
    if (grepl("^[0-9]$", ch) || (ch == "." && grepl("^[0-9]$", substr(text, i + 1L, i + 1L)))) {
      m <- regmatches(substr(text, i, n),
        regexec("^[0-9]+(\\.[0-9]*)?([eE][+-]?[0-9]+)?|^\\.[0-9]+([eE][+-]?[0-9]+)?",
          substr(text, i, n)))[[1]][1]
      if (is.na(m) || m == "") fail(paste0("bad number at: ", ch))
      toks[[length(toks) + 1]] <- list(kind = "number", text = m)
      i <- i + nchar(m); next
    }
    # identifier / keyword
    if (grepl("^[A-Za-z_]$", ch)) {
      m <- regmatches(substr(text, i, n),
        regexec("^[A-Za-z_][A-Za-z0-9_]*", substr(text, i, n)))[[1]][1]
      up <- toupper(m)
      toks[[length(toks) + 1]] <-
        if (up %in% PRED_RESERVED) list(kind = "keyword", text = up)
        else list(kind = "ident", text = m)
      i <- i + nchar(m); next
    }
    two <- substr(text, i, i + 1L)
    if (two %in% c("<>", "<=", ">=", "!=")) {
      toks[[length(toks) + 1]] <- list(kind = "op", text = two); i <- i + 2L; next
    }
    if (ch %in% c("=", "<", ">", "+", "-", "*", "/", "(", ")", ",", ".")) {
      toks[[length(toks) + 1]] <- list(kind = if (ch %in% c("(", ")", ",", ".")) "punct" else "op",
        text = ch)
      i <- i + 1L; next
    }
    fail(paste0("unexpected character: ", ch))
  }
  toks[[length(toks) + 1]] <- list(kind = "eof", text = "")
  toks
}

# parser state: an environment with toks and pos --------------------------
new_parser <- function(text) {
  e <- new.env(parent = emptyenv())
  e$toks <- tokenize(text); e$pos <- 1L; e$text <- text
  e
}
peek <- function(p, k = 0L) p$toks[[p$pos + k]]
next_tok <- function(p) { t <- peek(p); p$pos <- p$pos + 1L; t }
expect <- function(p, kind, text = NULL) {
  t <- peek(p)
  if (t$kind != kind || (!is.null(text) && t$text != text))
    yamaa_error("parse_failure",
      paste0("expected ", kind, if (!is.null(text)) paste0(" '", text, "'") else "",
        " but found '", t$text, "' in: ", p$text))
  next_tok(p)
}
at_keyword <- function(p, kw, k = 0L) {
  t <- peek(p, k); t$kind == "keyword" && t$text == kw
}

# ---- shared identifier: name ["." name]; qualified may use keywords -----
parse_identifier <- function(p) {
  head <- peek(p)
  if (head$kind == "ident") { next_tok(p); q <- NULL; nm <- head$text }
  else if (head$kind == "keyword") {
    # a reserved word can never be a bare identifier (REQ-0039)
    yamaa_error("parse_failure",
      paste0("reserved word cannot be an identifier: ", head$text, " in: ", p$text))
  } else yamaa_error("parse_failure",
      paste0("expected identifier in: ", p$text))
  if (peek(p)$kind == "punct" && peek(p)$text == ".") {
    next_tok(p)
    t2 <- next_tok(p)
    if (!(t2$kind %in% c("ident", "keyword")))
      yamaa_error("parse_failure", paste0("expected name after '.' in: ", p$text))
    q <- nm; nm <- t2$text
  }
  list(kind = "id", qualifier = q, name = nm)
}

id_to_string <- function(node) {
  if (!is.null(node$qualifier)) paste0(node$qualifier, ".", node$name) else node$name
}

# ---- literals -----------------------------------------------------------
parse_predicate_literal <- function(p) {
  t <- peek(p)
  if (t$kind == "string") { next_tok(p); return(list(kind = "lit", vtype = "str", v = t$text)) }
  if (t$kind == "number" || (t$kind == "op" && t$text %in% c("+", "-") &&
      peek(p, 1L)$kind == "number")) {
    # sign is part of a predicate number literal
    s <- ""
    if (t$kind == "op") { s <- next_tok(p)$text }
    num <- expect(p, "number")
    txt <- paste0(s, num$text)
    val <- suppressWarnings(as.numeric(txt))
    if (is.na(val)) yamaa_error("parse_failure", paste0("bad number: ", txt))
    vt <- if (grepl("[.eE]", txt)) "float" else "int"
    return(list(kind = "lit", vtype = vt, v = txt))
  }
  if (at_keyword(p, "NULL")) { next_tok(p); return(list(kind = "lit", vtype = "null", v = NULL)) }
  if (at_keyword(p, "DATE") || at_keyword(p, "DATETIME")) {
    kw <- next_tok(p)$text
    st <- expect(p, "string")
    return(list(kind = "lit", vtype = tolower(kw), v = st$text))
  }
  yamaa_error("parse_failure", paste0("expected literal in: ", p$text))
}

# ---- predicate ----------------------------------------------------------
parse_predicate_text <- function(text) {
  tryCatch({
    p <- new_parser(text)
    node <- parse_disjunction(p)
    expect(p, "eof")
    node
  }, yamaa_error = function(e) {
    if (attr(e, "yamaa_condition") == "parse_failure")
      yamaa_error("invalid_predicate", conditionMessage(e))
    else stop(e)
  })
}

parse_disjunction <- function(p) {
  left <- parse_conjunction(p)
  while (at_keyword(p, "OR")) { next_tok(p); left <- list(kind = "or", l = left, r = parse_conjunction(p)) }
  left
}
parse_conjunction <- function(p) {
  left <- parse_negation(p)
  while (at_keyword(p, "AND")) { next_tok(p); left <- list(kind = "and", l = left, r = parse_negation(p)) }
  left
}
parse_negation <- function(p) {
  n <- 0L
  while (at_keyword(p, "NOT")) { next_tok(p); n <- n + 1L }
  node <- parse_boolean(p)
  for (i in seq_len(n)) node <- list(kind = "not", x = node)
  node
}
parse_boolean <- function(p) {
  if (at_keyword(p, "TRUE")) { next_tok(p); return(list(kind = "lit_true")) }
  if (at_keyword(p, "FALSE")) { next_tok(p); return(list(kind = "lit_false")) }
  if (peek(p)$kind == "punct" && peek(p)$text == "(") {
    next_tok(p); node <- parse_disjunction(p); expect(p, "punct", ")"); return(node)
  }
  parse_comparison_or_nulltest(p)
}
parse_comparison_or_nulltest <- function(p) {
  left <- parse_pred_operand(p)
  t <- peek(p)
  if (t$kind == "keyword" && t$text == "IS") {
    next_tok(p)
    neg <- FALSE
    if (at_keyword(p, "NOT")) { next_tok(p); neg <- TRUE }
    expect(p, "keyword", "NULL")
    return(list(kind = "isnull", x = left, neg = neg))
  }
  neg <- FALSE
  if (t$kind == "keyword" && t$text == "NOT" &&
      peek(p, 1L)$kind == "keyword" && peek(p, 1L)$text %in% c("IN", "BETWEEN", "LIKE")) {
    next_tok(p); neg <- TRUE
  }
  t <- peek(p)
  if (t$kind == "op" && t$text %in% c("=", "<>", "!=", "<", "<=", ">", ">=")) {
    op <- next_tok(p)$text
    right <- parse_pred_operand(p)
    return(list(kind = "cmp", op = op, l = left, r = right))
  }
  if (t$kind == "keyword" && t$text == "IN") {
    next_tok(p); expect(p, "punct", "(")
    items <- list(parse_pred_operand(p))
    while (peek(p)$kind == "punct" && peek(p)$text == ",") {
      next_tok(p); items[[length(items) + 1]] <- parse_pred_operand(p)
    }
    expect(p, "punct", ")")
    return(list(kind = "in", x = left, items = items, neg = neg))
  }
  if (t$kind == "keyword" && t$text == "BETWEEN") {
    next_tok(p)
    lo <- parse_pred_operand(p); expect(p, "keyword", "AND"); hi <- parse_pred_operand(p)
    return(list(kind = "between", x = left, lo = lo, hi = hi, neg = neg))
  }
  if (t$kind == "keyword" && t$text == "LIKE") {
    next_tok(p)
    pat <- parse_pred_operand(p)
    esc <- NULL
    if (at_keyword(p, "ESCAPE")) { next_tok(p); esc <- expect(p, "string")$text }
    return(list(kind = "like", x = left, pat = pat, esc = esc, neg = neg))
  }
  yamaa_error("parse_failure", paste0("expected comparison after operand in: ", p$text))
}

parse_pred_operand <- function(p) {
  t <- peek(p)
  if (t$kind %in% c("ident")) return(parse_identifier(p))
  # keyword-headed operand: DATE/DATETIME/NULL literal
  if (t$kind == "keyword" && t$text %in% c("DATE", "DATETIME", "NULL")) return(parse_predicate_literal(p))
  if (t$kind %in% c("string", "number")) return(parse_predicate_literal(p))
  if (t$kind == "op" && t$text %in% c("+", "-")) return(parse_predicate_literal(p))
  yamaa_error("parse_failure", paste0("expected operand in: ", p$text))
}

# ---- numeric (compute) ---------------------------------------------------
NUMERIC_FNS <- c("ABS","CEIL","FLOOR","TRUNC","SQRT","POWER","EXP","LN",
  "MOD","GREATEST","LEAST","NULLIF","COALESCE")

parse_compute_text <- function(text) {
  p <- new_parser(text)
  node <- parse_c_expr(p)
  # comparisons are prohibited in compute (REQ-0441)
  if (peek(p)$kind == "op" && peek(p)$text %in% c(">", "<", ">=", "<=", "=", "<>", "!="))
    yamaa_error("prohibited_construct",
      paste0("comparison not allowed in compute: ", peek(p)$text, " in: ", text))
  expect(p, "eof")
  node
}

parse_c_expr <- function(p) parse_binop(p, parse_c_term, c("+", "-"))
parse_c_term <- function(p) parse_binop(p, parse_c_factor, c("*", "/"))
parse_binop <- function(p, sub, ops) {
  left <- sub(p)
  while (peek(p)$kind == "op" && peek(p)$text %in% ops) {
    op <- next_tok(p)$text
    left <- list(kind = "binop", op = op, l = left, r = sub(p))
  }
  left
}
parse_c_factor <- function(p) {
  if (peek(p)$kind == "op" && peek(p)$text %in% c("+", "-")) {
    op <- next_tok(p)$text
    return(list(kind = "unary", op = op, x = parse_c_primary(p)))
  }
  parse_c_primary(p)
}
parse_c_primary <- function(p) {
  t <- peek(p)
  if (t$kind == "number") {
    next_tok(p)
    return(list(kind = "lit", vtype = if (grepl("[.eE]", t$text)) "float" else "int", v = t$text))
  }
  if (at_keyword(p, "NULL")) { next_tok(p); return(list(kind = "lit", vtype = "null", v = NULL)) }
  if (peek(p)$kind == "punct" && peek(p)$text == "(") {
    next_tok(p); node <- parse_c_expr(p); expect(p, "punct", ")"); return(node)
  }
  if (t$kind == "ident") {
    # call or identifier: lookahead
    if (peek(p, 1L)$kind == "punct" && peek(p, 1L)$text == "(") {
      fn <- toupper(next_tok(p)$text)
      if (!fn %in% NUMERIC_FNS)
        yamaa_error("prohibited_function",
          paste0("function not allowed in compute: ", fn, " in: ", p$text))
      expect(p, "punct", "(")
      args <- list()
      if (!(peek(p)$kind == "punct" && peek(p)$text == ")")) {
        args[[1]] <- parse_c_expr(p)
        while (peek(p)$kind == "punct" && peek(p)$text == ",") {
          next_tok(p); args[[length(args) + 1]] <- parse_c_expr(p)
        }
      }
      expect(p, "punct", ")")
      return(list(kind = "call", fn = fn, args = args))
    }
    return(parse_identifier(p))
  }
  # a reserved word here is a grammar violation (prohibited table)
  if (t$kind == "keyword")
    yamaa_error("parse_failure", paste0("reserved word not allowed in compute: ", t$text))
  yamaa_error("parse_failure", paste0("expected primary in: ", p$text))
}

# ---- aggregate -----------------------------------------------------------
AGGREGATE_FNS <- NUMERIC_FNS  # aggregate imports numeric's function vocabulary
REDUCERS <- c("SUM", "COUNT", "MIN", "MAX", "MEAN", "ONLY")

parse_aggregate_text <- function(text) {
  p <- new_parser(text)
  node <- parse_a_expr(p)
  expect(p, "eof")
  node
}

parse_a_expr <- function(p) parse_a_binop(p, parse_a_term, c("+", "-"))
parse_a_term <- function(p) parse_a_binop(p, parse_a_factor, c("*", "/"))
parse_a_binop <- function(p, sub, ops) {
  left <- sub(p)
  while (peek(p)$kind == "op" && peek(p)$text %in% ops) {
    op <- next_tok(p)$text
    left <- list(kind = "binop", op = op, l = left, r = sub(p))
  }
  left
}
parse_a_factor <- function(p) {
  if (peek(p)$kind == "op" && peek(p)$text %in% c("+", "-")) {
    op <- next_tok(p)$text
    return(list(kind = "unary", op = op, x = parse_a_primary(p)))
  }
  parse_a_primary(p)
}
parse_a_primary <- function(p) {
  t <- peek(p)
  if (t$kind == "number") {
    next_tok(p)
    return(list(kind = "lit", vtype = if (grepl("[.eE]", t$text)) "float" else "int", v = t$text))
  }
  if (at_keyword(p, "NULL")) { next_tok(p); return(list(kind = "lit", vtype = "null", v = NULL)) }
  if (peek(p)$kind == "punct" && peek(p)$text == "(") {
    next_tok(p); node <- parse_a_expr(p); expect(p, "punct", ")"); return(node)
  }
  if (t$kind == "ident") {
    nm <- t$text
    up <- toupper(nm)
    if (peek(p, 1L)$kind == "punct" && peek(p, 1L)$text == "(") {
      next_tok(p)  # consume name
      expect(p, "punct", "(")
      if (up %in% REDUCERS) {
        # reduction: reducer "(" (expr | star) ")"
        if (peek(p)$kind == "ident" && peek(p, 1L)$kind == "punct" && peek(p, 1L)$text == "." &&
            peek(p, 2L)$kind == "op" && peek(p, 2L)$text == "*") {
          q <- next_tok(p)$text; next_tok(p); next_tok(p)
          expect(p, "punct", ")")
          if (up != "COUNT")
            yamaa_error("parse_failure", "only COUNT may take a star argument")
          return(list(kind = "reduce", reducer = up, star = q))
        }
        arg <- parse_a_expr(p)
        expect(p, "punct", ")")
        return(list(kind = "reduce", reducer = up, arg = arg))
      }
      if (up %in% AGGREGATE_FNS) {
        args <- list()
        if (!(peek(p)$kind == "punct" && peek(p)$text == ")")) {
          args[[1]] <- parse_a_expr(p)
          while (peek(p)$kind == "punct" && peek(p)$text == ",") {
            next_tok(p); args[[length(args) + 1]] <- parse_a_expr(p)
          }
        }
        expect(p, "punct", ")")
        return(list(kind = "call", fn = up, args = args))
      }
      yamaa_error("parse_failure", paste0("unknown function or reducer: ", nm))
    }
    return(parse_identifier(p))
  }
  if (t$kind == "keyword")
    yamaa_error("parse_failure", paste0("reserved word not allowed in aggregate: ", t$text))
  yamaa_error("parse_failure", paste0("expected primary in: ", p$text))
}

# ---- predicate evaluator -------------------------------------------------
# eval_pred(node, resolve) -> logical vector (NA = UNKNOWN); resolve(name) -> tv
eval_pred <- function(node, resolve) {
  k <- node$kind
  if (k == "lit_true") return(rep(TRUE, resolve_n(resolve)))
  if (k == "lit_false") return(rep(FALSE, resolve_n(resolve)))
  if (k == "or") return(tv_or(eval_pred(node$l, resolve), eval_pred(node$r, resolve)))
  if (k == "and") return(tv_and(eval_pred(node$l, resolve), eval_pred(node$r, resolve)))
  if (k == "not") return(tv_not(eval_pred(node$x, resolve)))
  if (k == "isnull") {
    x <- resolve_operand(node$x, resolve)
    r <- is.na(x$v)
    if (node$neg) r <- !r
    return(r)
  }
  if (k == "cmp") {
    l <- resolve_operand(node$l, resolve); r <- resolve_operand(node$r, resolve)
    c <- cmp_typed(l, r)
    out <- switch(node$op,
      "=" = c == 0, "<>" = c != 0, "!=" = c != 0, "<" = c < 0, "<=" = c <= 0,
      ">" = c > 0, ">=" = c >= 0)
    out[is.na(c)] <- NA
    return(out)
  }
  if (k == "in") {
    x <- resolve_operand(node$x, resolve)
    # IN: TRUE if any equal non-missing; FALSE if all non-missing and none
    # equal; UNKNOWN if no equal and some side is missing (REQ-0090)
    hits <- rep(FALSE, length(x$v)); unk <- rep(FALSE, length(x$v))
    for (it in node$items) {
      iv <- resolve_operand(it, resolve)
      c <- cmp_typed(x, iv)
      hits <- hits | (!is.na(c) & c == 0)
      unk <- unk | is.na(c)
    }
    out <- ifelse(hits, TRUE, ifelse(unk, NA, FALSE))
    if (node$neg) out <- tv_not(out)
    return(out)
  }
  if (k == "between") {
    lo <- resolve_operand(node$lo, resolve); hi <- resolve_operand(node$hi, resolve)
    x <- resolve_operand(node$x, resolve)
    c1 <- cmp_typed(x, lo); c2 <- cmp_typed(x, hi)
    ge <- c1 >= 0; ge[is.na(c1)] <- NA
    le <- c2 <= 0; le[is.na(c2)] <- NA
    out <- tv_and(ge, le)
    if (node$neg) out <- tv_not(out)
    return(out)
  }
  if (k == "like") {
    x <- resolve_operand(node$x, resolve)
    if (x$t != "str") yamaa_error("incompatible_input_type", "LIKE needs str")
    pat <- resolve_operand(node$pat, resolve)
    if (pat$t != "str") yamaa_error("incompatible_input_type", "LIKE pattern needs str")
    out <- mapply(function(s, pp) {
      if (is.na(s) || is.na(pp)) return(NA)
      rx <- like_to_regex(pp, node$esc)
      grepl(rx, s, perl = TRUE)
    }, x$v, pat$v, USE.NAMES = FALSE)
    out <- as.logical(out)
    if (node$neg) out <- tv_not(out)
    return(out)
  }
  yamaa_error("invalid_argument", paste0("unknown predicate node: ", k))
}

# resolve() returns a list(resolve = fn(name)->tv, n = <row count>)
resolve_n <- function(r) r$n
resolve_operand <- function(node, resolve) {
  if (node$kind == "id") return(resolve$resolve(id_to_string(node)))
  if (node$kind == "lit") {
    n <- resolve$n
    if (node$vtype == "null") return(tv_na("str", n))  # type-neutral missing
    if (node$vtype == "str") return(tv(rep(node$v, n), "str"))
    if (node$vtype == "int") return(tv(rep(as.integer(node$v), n), "int"))
    if (node$vtype == "float") return(tv(rep(as.numeric(node$v), n), "float"))
    if (node$vtype == "date") return(tv(rep(parse_temporal_one(node$v, "date")$canon, n), "date"))
    if (node$vtype == "datetime") return(tv(rep(parse_temporal_one(node$v, "datetime")$canon, n), "datetime"))
  }
  yamaa_error("invalid_argument", "bad operand")
}

# LIKE pattern -> regex; escape char defaults: backslash escapes only when an
# explicit ESCAPE clause names it (REQ-0091..0093)
like_to_regex <- function(pat, esc) {
  chars <- strsplit(pat, "", fixed = TRUE)[[1]]
  out <- "^"; i <- 1L
  while (i <= length(chars)) {
    ch <- chars[i]
    if (!is.null(esc) && ch == esc && i < length(chars)) {
      out <- paste0(out, regex_escape(chars[i + 1L])); i <- i + 2L; next
    }
    if (ch == "%") { out <- paste0(out, ".*"); i <- i + 1L; next }
    if (ch == "_") { out <- paste0(out, "."); i <- i + 1L; next }
    out <- paste0(out, regex_escape(ch)); i <- i + 1L
  }
  paste0(out, "$")
}
regex_escape <- function(ch) {
  if (grepl("^[A-Za-z0-9 ]$", ch)) ch else paste0("\\", ch)
}

# ---- compute evaluator ----------------------------------------------------
# eval_compute(node, resolve) -> tv (numeric type or NULL-literal "null")
eval_compute <- function(node, resolve) {
  k <- node$kind
  if (k == "lit") {
    n <- resolve$n
    if (node$vtype == "null") return(tv_na("float", n))  # missing numeric
    if (node$vtype == "int") return(tv(rep(as.integer(node$v), n), "int"))
    return(tv(rep(as.numeric(node$v), n), "float"))
  }
  if (k == "id") return(resolve$resolve(id_to_string(node)))
  if (k == "unary") {
    x <- eval_compute(node$x, resolve)
    neg <- node$op == "-"
    if (!neg) return(x)
    return(arith_unary_minus(x))
  }
  if (k == "binop") {
    l <- eval_compute(node$l, resolve); r <- eval_compute(node$r, resolve)
    return(arith_binop(node$op, l, r))
  }
  if (k == "call") {
    ev <- lapply(node$args, function(a) eval_compute(a, resolve))
    return(eval_num_fn(node$fn, ev))
  }
  yamaa_error("invalid_argument", paste0("unknown compute node: ", k))
}

arith_unary_minus <- function(x) {
  if (!x$t %in% c("int", "float"))
    yamaa_error("incompatible_input_type", "unary minus needs a numeric")
  v <- x$v
  if (x$t == "int") {
    out <- rep(NA_integer_, length(v))
    ok <- !is.na(v) & v != -.Machine$integer.max  # -(-2^31) overflows
    out[ok] <- -v[ok]
    if (any(!is.na(v) & !ok)) yamaa_error("overflow", "integer negation overflow")
    return(tv(out, "int"))
  }
  tv(-v, "float")
}

# REQ-0017: numeric promotion; int+int stays int, else float.
arith_binop <- function(op, l, r) {
  if (!l$t %in% c("int", "float") || !r$t %in% c("int", "float"))
    yamaa_error("incompatible_input_type", "arithmetic needs numerics")
  both_int <- l$t == "int" && r$t == "int"
  lv <- as.numeric(l$v); rv <- as.numeric(r$v)
  n <- max(length(lv), length(rv))
  lv <- rep(lv, length.out = n); rv <- rep(rv, length.out = n)
  out <- rep(NA_real_, n)
  ok <- !is.na(lv) & !is.na(rv)
  raw <- switch(op,
    "+" = lv[ok] + rv[ok], "-" = lv[ok] - rv[ok],
    "*" = lv[ok] * rv[ok], "/" = lv[ok] / rv[ok])
  # division by zero is an error (REQ-0441), not Inf
  if (op == "/" && any(rv[ok] == 0))
    yamaa_error("division_by_zero", "division by zero")
  if (both_int && op != "/") {
    # int arithmetic: detect overflow, values must stay integral
    bad <- !is.finite(raw) | raw != floor(raw) | abs(raw) > .Machine$integer.max
    if (any(bad)) yamaa_error("overflow", "integer arithmetic overflow")
    out[ok] <- raw
    return(tv(as.integer(out), "int"))
  }
  out[ok] <- raw
  tv(out, "float")
}

# numeric functions over already-evaluated argument tvs ----------------------
eval_num_fn <- function(fn, ev) {
  need_num <- function(x) {
    if (!x$t %in% c("int", "float"))
      yamaa_error("incompatible_input_type", paste0(fn, " needs numerics"))
    x
  }
  n <- max(vapply(ev, tv_len, integer(1)))
  num1 <- function() as.numeric(need_num(ev[[1]])$v)
  switch(fn,
    ABS = { x <- need_num(ev[[1]]); tv(abs(x$v), x$t) },
    CEIL = { x <- need_num(ev[[1]]); tv(ceiling(as.numeric(x$v)), "float") },
    FLOOR = { x <- need_num(ev[[1]]); tv(floor(as.numeric(x$v)), "float") },
    TRUNC = { x <- need_num(ev[[1]]); tv(trunc(as.numeric(x$v)), "float") },
    SQRT = { x <- num1(); if (any(x < 0, na.rm = TRUE)) yamaa_error("sqrt_of_negative", "SQRT of negative"); tv(sqrt(x), "float") },
    EXP = { tv(exp(num1()), "float") },
    LN = { x <- num1(); if (any(x <= 0, na.rm = TRUE)) yamaa_error("ln_of_nonpositive", "LN of non-positive"); tv(log(x), "float") },
    POWER = { tv(num1() ^ as.numeric(need_num(ev[[2]])$v), "float") },
    MOD = { a <- num1(); b <- as.numeric(need_num(ev[[2]])$v);
            if (any(b == 0, na.rm = TRUE)) yamaa_error("division_by_zero", "MOD by zero");
            tv(a - b * trunc(a / b), "float") },  # sign of x per REQ-0415
    GREATEST = , LEAST = {
      xs <- lapply(ev, function(x) as.numeric(need_num(x)$v))
      # REQ-0415: largest/smallest non-NULL argument; NULL only if all are
      m <- do.call(if (fn == "GREATEST") pmax else pmin, c(xs, list(na.rm = TRUE)))
      all_na <- Reduce(`&`, lapply(xs, is.na))
      m[all_na] <- NA_real_
      tv(m, "float")
    },
    NULLIF = {
      a <- num1(); b <- as.numeric(need_num(ev[[2]])$v)
      out <- a; out[!is.na(a) & !is.na(b) & a == b] <- NA_real_
      tv(out, "float")
    },
    COALESCE = {
      # first non-missing; mixed int/float -> float
      xs <- lapply(ev, need_num)
      t <- if (all(vapply(xs, function(x) x$t, character(1)) == "int")) "int" else "float"
      acc <- rep(NA_real_, n)
      for (x in xs) {
        xv <- as.numeric(x$v); acc[is.na(acc)] <- xv[is.na(acc)]
      }
      if (t == "int") tv(as.integer(acc), "int") else tv(acc, "float")
    },
    yamaa_error("invalid_argument", paste0("unknown function: ", fn)))
}

# round_half_away_from_zero (REQ-0444): vectorized --------------------------
round_half_away <- function(x, digits) {
  v <- x$v
  if (!x$t %in% c("int", "float"))
    yamaa_error("incompatible_input_type", "ROUND needs a numeric")
  f <- as.numeric(v)
  p <- 10 ^ digits
  out <- sign(f) * floor(abs(f) * p + 0.5) / p
  out[is.na(f)] <- NA_real_
  # a value that rounds to zero returns positive zero
  out[out == 0] <- 0
  tv(out, "float")
}
