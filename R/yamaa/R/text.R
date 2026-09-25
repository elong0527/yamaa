# text.R -- Text operations (rules/operations/text.md) and text grammar support.
# All text operations run under LC_COLLATE=C so case conversion follows the
# ASCII mapping (REQ-0608) and comparisons are scalar-value order (REQ-0026).

ascii_upper <- function(v) chartr("abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", v)
ascii_lower <- function(v) chartr("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", v)

# str_upper/str_lower require string sources (REQ-0308). NA stays NA.
op_str_case <- function(src, upper) {
  t <- if (src$t == "str") "str"
       else yamaa_error("incompatible_input_type", paste0("str case source type: ", src$t))
  v <- to_canon_text(src)
  out <- ifelse(is.na(v), NA_character_, if (upper) ascii_upper(v) else ascii_lower(v))
  tv(out, "str")
}

# str_sentence/str_title: ASCII-only scalar transforms (REQ-1240/1241).
# shared ASCII-only str transform driver: type check + per-value mapping
str_word_op <- function(src, kind, one) {
  if (src$t != "str")
    yamaa_error("incompatible_input_type", paste0(kind, " source must be str"))
  tv(vapply(src$v, one, character(1), USE.NAMES = FALSE), "str")
}

op_str_sentence <- function(src) str_word_op(src, "str_sentence", sentence_case_one)

sentence_case_one <- function(s) {
  if (is.na(s)) return(NA_character_)
  chars <- strsplit(s, "", fixed = TRUE)[[1]]
  if (length(chars) == 0) return(s)
  paste0(c(ascii_upper(chars[1]), ascii_lower(chars[-1])), collapse = "")
}

op_str_title <- function(src) str_word_op(src, "str_title", title_case_one)

title_case_one <- function(s) {
  if (is.na(s)) return(NA_character_)
  chars <- strsplit(s, "", fixed = TRUE)[[1]]
  res <- chars
  for (i in seq_along(chars)) {
    if (!grepl("^[A-Za-z]$", chars[i])) next
    res[i] <- if (i > 1 && grepl("^[A-Za-z]$", chars[i - 1])) ascii_lower(chars[i])
              else ascii_upper(chars[i])
  }
  paste0(res, collapse = "")
}

to_canon_text <- function(src) {
  switch(src$t,
    str = src$v,
    int = ifelse(is.na(src$v), NA_character_, sprintf("%.0f", src$v)),
    float = float_text(src$v),
    date = src$v, datetime = src$v)
}

# REQ-0033 word separators (ASCII whitespace only)
is_word_char <- function(ch) grepl("^[A-Za-z0-9_]$", ch)

# split into an ordered token list of list(kind = "word"|"sep", text)
tokenize_words <- function(s) {
  chars <- strsplit(s, "", fixed = TRUE)[[1]]
  toks <- list(); cur <- ""; in_word <- FALSE
  flush <- function() {
    if (nchar(cur) > 0 || in_word) {
      toks[[length(toks) + 1]] <<- list(kind = if (in_word) "word" else "sep", text = cur)
      cur <<- ""; in_word <<- FALSE
    }
  }
  for (ch in chars) {
    w <- is_word_char(ch)
    if (w != in_word && nchar(cur) > 0) flush()
    cur <- paste0(cur, ch); in_word <- w
  }
  flush()
  toks
}

# validate a regex pattern, failing invalid_regex (REQ-0827); predicate
# parse sites pass cond="invalid_predicate" (REQ-1244)
# REQ-0826: the portable grammar rejects `(?P<name>` named groups, inline
assert_valid_regex <- function(pattern, cond = "invalid_regex") {
  ok <- tryCatch({ grepl(pattern, "", perl = TRUE); TRUE },
                 error = function(e) FALSE)
  if (ok) {
    # portable-grammar rejections (REQ-0826)
    if (grepl("(?P<", pattern, fixed = TRUE)) ok <- FALSE
    else if (grepl("(?i)", pattern, fixed = TRUE)) ok <- FALSE
    else if (grepl("\\p{", pattern, fixed = TRUE)) ok <- FALSE
  }
  if (!ok) yamaa_error(cond, paste0("invalid regex: ", pattern))
  invisible(NULL)
}

# drop [...] character classes from a regex: a '(' inside a class is a
# literal, not a capturing group. A ']' immediately after the opening '['
# is a POSIX literal, not the class end. Escaped pairs are consumed whole,
# so an escaped bracket never opens or closes a class (e.g. '[\]]' stays
# one class); stripping escapes first would collapse it to '[]' and leave
# the class unterminated.
strip_regex_classes <- function(p) {
  ch <- strsplit(p, "", fixed = TRUE)[[1]]
  keep <- logical(length(ch))
  in_class <- FALSE; first <- FALSE
  i <- 1L
  while (i <= length(ch)) {
    c <- ch[i]
    if (c == "\\" && i < length(ch)) {
      # escaped pair: never a class bracket or a group paren. Inside a
      # class it is a member, so a following ']' closes the class.
      if (!in_class) { keep[i] <- TRUE; keep[i + 1L] <- TRUE }
      else first <- FALSE
      i <- i + 2L
      next
    }
    if (!in_class) {
      if (c == "[") { in_class <- TRUE; first <- TRUE }
      else keep[i] <- TRUE
    } else if (first && c == "]") {
      first <- FALSE  # literal ']' inside the class
    } else if (c == "]") {
      in_class <- FALSE
    } else {
      first <- FALSE
    }
    i <- i + 1L
  }
  paste(ch[keep], collapse = "")
}

# str_extract(source, pattern, group, missing, invalid)
op_str_extract <- function(src, pattern, group, missing_h = NA_character_, invalid_h = NA_character_, no_match_h = NA_character_) {
  if (src$t != "str") yamaa_error("incompatible_input_type", "str_extract source must be str")
  assert_valid_regex(pattern)
  # REQ-0828: group must not exceed the pattern's capturing-group count.
  # count capturing groups syntactically: '(' not followed by '?' except
  # '(?P<name>' which captures. Classes are stripped first with an
  # escape-aware scan (so '[\]]' does not collapse), then escaped chars
  # are dropped so '\(' is not counted.
  p2 <- strip_regex_classes(pattern)
  p2 <- gsub("\\\\.", "", p2)  # drop escaped chars
  # remove non-capturing constructs
  p2 <- gsub("\\(\\?:|\\(\\?=|\\(\\?!|\\(\\?<=|\\(\\?<!|\\(\\?>|\\(\\?#", "", p2)
  # a '(' inside a character class is a literal, not a group
  gc <- lengths(regmatches(p2, gregexpr("(", p2, fixed = TRUE)))[1]
  if (group > gc)
    yamaa_error("regex_group_out_of_range",
      paste0("group ", group, " exceeds group count ", gc, " in: ", pattern))
  vapply(src$v, function(s) {
    if (is.na(s)) return(missing_h)
    ok <- tryCatch({ grepl(pattern, s, perl = TRUE); TRUE },
                   error = function(e) FALSE)
    if (!ok) return(invalid_h)
    if (!grepl(pattern, s, perl = TRUE)) return(no_match_h)
    m <- regexec(pattern, s, perl = TRUE)
    caps <- regmatches(s, m)[[1]]
    # caps[1] is whole match; group g is caps[g+1]
    idx <- group + 1L
    if (idx > length(caps) || is.na(caps[idx])) missing_h else caps[idx]
  }, character(1), USE.NAMES = FALSE) |> tv("str")
}

# str_pad(source, width): left-pad canonical text to a minimum width with
# ASCII spaces (REQ-1261). Missing stays missing; a non-positive-integer
# width fails invalid_field_type; an unconvertible source reports the
# conversion's own condition.
op_str_pad <- function(src, width) {
  if (!is.numeric(width) || length(width) != 1 || is.na(width) ||
      width != floor(width) || width < 1)
    yamaa_error("invalid_field_type",
      paste0("str_pad width must be a positive integer, got: ", deparse1(width)))
  s <- convert_tv(src, "str")$v
  out <- rep(NA_character_, length(s))
  ok <- !is.na(s)
  pad <- pmax(0L, as.integer(width) - nchar(s[ok]))
  out[ok] <- paste0(strrep(" ", pad), s[ok])
  tv(out, "str")
}

# str_contains(source, pattern, missing): "true"/"false" strings (REQ-1243)
op_str_contains <- function(src, pattern, missing_h = NA_character_) {
  assert_valid_regex(pattern)
  str_word_op(src, "str_contains", function(s) {
    if (is.na(s)) return(missing_h)
    if (grepl(pattern, s, perl = TRUE)) "true" else "false"
  })
}

# str_concat(sources, separator, missing): missing -> the missing literal;
# NA -> missing literal (REQ-1112)
op_str_concat <- function(srcs, separator, missing_h = NA_character_) {
  n <- length(srcs[[1]]$v)
  out <- character(n)
  for (i in seq_len(n)) {
    parts <- vapply(srcs, function(s) {
      v <- to_canon_text(s)
      w <- v[i]
      if (is.na(w)) missing_h else w
    }, character(1))
    if (any(is.na(parts))) { out[i] <- NA_character_; next }
    out[i] <- paste(parts, collapse = separator)
  }
  tv(out, "str")
}

# str_template(template, arguments, missing, invalid)
# (REQ-0449..0454, contract string-template). Scan left to right: brace pairs
# must begin or end a valid placeholder (REQ-0454); anything else fails
# invalid_string_template (REQ-0461).
parse_string_template <- function(template) {
  ident <- "[A-Za-z_][A-Za-z0-9_]*"
  var_re <- paste0("^", ident, "(\\.", ident, ")?$")
  parts <- list()
  buf <- ""
  i <- 1L
  n <- nchar(template)
  flush <- function() {
    if (nchar(buf) > 0) parts[[length(parts) + 1L]] <<- list(kind = "text", text = buf)
    buf <<- ""
  }
  while (i <= n) {
    ch <- substr(template, i, i)
    if (ch == "{") {
      if (i < n && substr(template, i + 1L, i + 1L) == "{") {
        buf <- paste0(buf, "{"); i <- i + 2L; next
      }
      j <- regexpr("}", substr(template, i, n), fixed = TRUE)[1]
      if (j < 0)
        yamaa_error("invalid_string_template",
          paste0("unmatched brace in template: ", template))
      j <- i + j - 1L
      body <- substr(template, i + 1L, j - 1L)
      if (!grepl(var_re, body, perl = TRUE))
        yamaa_error("invalid_string_template",
          paste0("invalid placeholder {", body, "} in template: ", template))
      flush()
      parts[[length(parts) + 1L]] <- list(kind = "placeholder", name = body)
      i <- j + 1L
      next
    }
    if (ch == "}") {
      if (i < n && substr(template, i + 1L, i + 1L) == "}") {
        buf <- paste0(buf, "}"); i <- i + 2L; next
      }
      yamaa_error("invalid_string_template",
        paste0("unmatched brace in template: ", template))
    }
    buf <- paste0(buf, ch)
    i <- i + 1L
  }
  flush()
  parts
}

# the distinct placeholder names in a parsed template, in scan order
template_placeholders <- function(parts)
  unique(unlist(lapply(parts, function(p)
    if (p$kind == "placeholder") p$name)))

op_str_template <- function(parts, args, missing_h = NA_character_,
    invalid_h = NA_character_, n) {
  names_needed <- template_placeholders(parts)
  out <- character(n)
  for (i in seq_len(n)) {
    s <- ""
    st <- "ok"
    for (p in parts) {
      if (p$kind == "text") {
        s <- paste0(s, p$text)
        next
      }
      a <- args[[p$name]]
      if (is.null(a)) {
        st <- "invalid"
        break
      }  # REQ-0462 surfaces as unknown_field at binding time
      v <- to_canon_text(a)[i]
      if (is.na(v)) {
        st <- "missing"
        break
      }  # REQ-0459/1113: missing placeholder value -> declared missing
      s <- paste0(s, v)
    }
    out[i] <- switch(st, ok = s, missing = missing_h, invalid = invalid_h)
  }
  tv(out, "str")
}

is_sep <- function(ch) !is_word_char(ch)
