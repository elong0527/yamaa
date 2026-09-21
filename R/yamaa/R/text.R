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

to_canon_text <- function(src) {
  switch(src$t,
    str = src$v,
    int = ifelse(is.na(src$v), NA_character_, as.character(src$v)),
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

# validate a regex pattern, failing invalid_regex (REQ-0827)
# REQ-0826: the portable grammar rejects `(?P<name>` named groups, inline
# flag groups like `(?i)`, and `\p{...}` property escapes -- not just
# patterns the host library cannot compile
assert_valid_regex <- function(pattern) {
  ok <- tryCatch({ grepl(pattern, "", perl = TRUE); TRUE },
                 error = function(e) FALSE)
  if (ok) {
    # portable-grammar rejections (REQ-0826)
    if (grepl("(?P<", pattern, fixed = TRUE)) ok <- FALSE
    else if (grepl("(?i)", pattern, fixed = TRUE)) ok <- FALSE
    else if (grepl("\\p{", pattern, fixed = TRUE)) ok <- FALSE
  }
  if (!ok) yamaa_error("invalid_regex", paste0("invalid regex: ", pattern))
  invisible(NULL)
}

# str_extract(source, pattern, group, missing, invalid)
op_str_extract <- function(src, pattern, group, missing_h = NA_character_, invalid_h = NA_character_, no_match_h = NA_character_) {
  if (src$t != "str") yamaa_error("incompatible_input_type", "str_extract source must be str")
  assert_valid_regex(pattern)
  # REQ-0828: group must not exceed the pattern's capturing-group count.
  # count capturing groups syntactically: '(' not followed by '?' except
  # '(?P<name>' which captures; ignore escaped parens.
  p2 <- gsub("\\\\.", "", pattern)  # drop escaped chars
  # remove non-capturing constructs
  p2 <- gsub("\\(\\?:|\\(\\?=|\\(\\?!|\\(\\?<=|\\(\\?<!|\\(\\?>|\\(\\?#", "", p2)
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
# The closed template grammar lives in WT/yaml/grammar/string-template.yaml
# (REQ-0449..0454, contract string-template). Scan left to right: brace pairs
# take precedence, then placeholders, then text. Returns the list of parts:
# list(kind = "text", text = ...) | list(kind = "placeholder", name = ...).
# A placeholder body must be a `variable`: identifier ["." identifier]
# (predicate.yaml: name ["." name]; whitespace is not ignored). Every brace
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

# title case (REQ-0611): uppercase each word's first letter, lowercase the rest
op_title_case <- function(src) {
  v <- to_canon_text(src)
  vapply(v, function(s) {
    if (is.na(s)) return(NA_character_)
    toks <- tokenize_words(s)
    out <- vapply(toks, function(tk) {
      w <- tk$text
      if (tk$kind != "word" || nchar(w) == 0) return(w)
      paste0(ascii_upper(substr(w, 1, 1)), ascii_lower(substr(w, 2, nchar(w))))
    }, character(1))
    paste(out, collapse = "")
  }, character(1), USE.NAMES = FALSE) |> tv("str")
}

# sentence case (REQ-0610): uppercase sentence starters, lowercase the rest
op_sentence_case <- function(src) {
  v <- to_canon_text(src)
  vapply(v, function(s) {
    if (is.na(s)) return(NA_character_)
    t <- tokenize_words(s)
    s2 <- ascii_lower(s)
    chars <- strsplit(s2, "", fixed = TRUE)[[1]]
    out <- chars; prev <- ""
    for (k in seq_along(chars)) {
      ch <- chars[k]
      if (is_word_char(ch) && (k == 1 || prev %in% c(".", "!", "?") || is_sep(prev))) {
        out[k] <- ascii_upper(ch)
      }
      prev <- ch
    }
    paste(out, collapse = "")
  }, character(1), USE.NAMES = FALSE) |> tv("str")
}

is_sep <- function(ch) !is_word_char(ch)

# REQ-0034/0035: truncation and whitespace for fixed-width output ---------
truncate_text <- function(v, max_len) {
  vapply(v, function(s) if (is.na(s) || nchar(s) <= max_len) s else substr(s, 1, max_len),
    character(1), USE.NAMES = FALSE)
}
pad_trailing_ws <- function(v, width) {
  vapply(v, function(s) {
    if (is.na(s)) return(s)
    w <- nchar(s)
    if (w >= width) s else paste0(s, strrep(" ", width - w))
  }, character(1), USE.NAMES = FALSE)
}
