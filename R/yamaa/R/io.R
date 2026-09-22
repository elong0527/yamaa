# io.R -- storage profiles: CSV/parquet ingestion (rules/storage/csv.md,
# parquet.md, ingestion.md) and artifact writing (publication.md).
#
# An ingested dataset is a "ydf": a data.frame of storage vectors with a
# "coltypes" attribute (named character vector of yamaa types). Missing is NA.

# REQ-0716/0830: the path's extension selects the profile (closed mapping,
# case-insensitive). NULL names no profile.
profile_of_path <- function(path) {
  if (is.null(path) || length(path) == 0 || !nzchar(path)) return(NULL)
  ext <- tolower(tools::file_ext(path))
  if (ext == "csv") "csv" else if (ext == "parquet") "parquet" else NULL
}

# ---- CSV reading ----------------------------------------------------------
read_ydf_csv <- function(path, types = list(), empty_string = "missing") {
  # REQ-1161 is enforced by the caller: a delimited source never declares
  # the present convention, so the reader always treats empty as missing.
  raw <- read_raw_bytes(path)
  if (length(raw) >= 3 && raw[1] == 0xef && raw[2] == 0xbb && raw[3] == 0xbf)
    yamaa_error("invalid_csv", paste0("BOM not allowed: ", path))
  # REQ-0029: ill-formed encoded text fails with invalid_text at the entry
  # boundary. Validate the bytes before any character-level parsing (which
  # would choke on them), locating the offending record/field for the report.
  assert_utf8_bytes(raw, path)
  text <- rawToChar(raw)
  # mark UTF-8 so nchar and friends count characters, not bytes
  Encoding(text) <- "UTF-8"
  # lone CR check happens per-field; split records on CRLF/LF
  recs <- split_csv_records(text, path)
  if (length(recs) == 0) yamaa_error("invalid_csv", paste0("no header row: ", path))
  header <- recs[[1]]
  if (any(header == "")) yamaa_error("source_field_name_empty", paste0("empty header name: ", path))
  if (anyDuplicated(header)) yamaa_error("source_field_name_duplicate", paste0("duplicate header name: ", path))
  # REQ-0532: types declaring a field not present in the source fail
  for (h in names(types)) {
    if (!h %in% header)
      yamaa_error("unknown_field", paste0("types declares unknown field: ", h))
  }
  body <- recs[-1]
  # drop trailing empty record from a final newline: split_csv_records
  # returns the last line even if empty; remove it only if fully empty
  if (length(body) > 0 && all(body[[length(body)]] == ""))
    body <- body[-length(body)]
  ncols <- length(header)
  cols <- vector("list", ncols); names(cols) <- header
  for (j in seq_len(ncols)) cols[[j]] <- character(0)
  for (i in seq_along(body)) {
    f <- body[[i]]
    if (length(f) != ncols)
      yamaa_error("source_record_width",
        paste0("row ", i + 1, " has ", length(f), " fields, header has ", ncols, ": ", path))
    for (j in seq_len(ncols)) cols[[j]] <- c(cols[[j]], f[j])
  }
  # empty field -> missing; otherwise keep text
  for (j in seq_len(ncols)) {
    v <- cols[[j]]; v[v == ""] <- NA_character_
    cols[[j]] <- v
  }
  df <- as.data.frame(cols, stringsAsFactors = FALSE, check.names = FALSE)
  coltypes <- vapply(header, function(h) {
    t <- types[[h]]
    if (is.null(t)) "str" else t
  }, character(1), USE.NAMES = FALSE)
  names(coltypes) <- header
  typed <- lapply(header, function(h) apply_column_type(df[[h]], coltypes[[h]], path, h))
  df2 <- as.data.frame(typed, stringsAsFactors = FALSE, check.names = FALSE)
  names(df2) <- header
  attr(df2, "coltypes") <- coltypes
  df2
}

read_raw_bytes <- function(path) {
  con <- file(path, "rb"); on.exit(close(con))
  readBin(con, "raw", n = file.info(path)$size)
}

# REQ-0029: fail with invalid_text on ill-formed UTF-8 bytes, reporting the
# 1-based record and field. Byte-level: safe on invalid input, mirrors the
# parser's quoting ("" inside quotes is an escaped quote).
assert_utf8_bytes <- function(raw, path) {
  t0 <- rawToChar(raw)
  if (validUTF8(t0)) return(invisible(NULL))
  nl <- which(raw == as.raw(0x0a))
  starts <- c(1L, nl + 1L); ends <- c(nl - 1L, length(raw))
  for (i in seq_along(starts)) {
    if (starts[i] > ends[i]) next
    lr <- raw[starts[i]:ends[i]]
    if (!validUTF8(rawToChar(lr))) {
      yamaa_error("invalid_text", paste0("ill-formed UTF-8 in ", path,
        ": record ", i, ", field ", utf8_bad_field(lr)))
    }
  }
  yamaa_error("invalid_text", paste0("ill-formed UTF-8 in ", path))
}

utf8_bad_field <- function(lr) {
  # split the line's bytes into fields at commas outside quotes
  bounds <- list(); fstart <- 1L; in_q <- FALSE
  n <- length(lr); k <- 1L
  while (k <= n) {
    b <- lr[k]
    if (in_q) {
      if (b == as.raw(0x22)) {
        if (k < n && lr[k + 1L] == as.raw(0x22)) { k <- k + 2L; next }
        in_q <- FALSE
      }
      k <- k + 1L; next
    }
    if (b == as.raw(0x22)) { in_q <- TRUE; k <- k + 1L; next }
    if (b == as.raw(0x2c)) {
      bounds[[length(bounds) + 1L]] <- c(fstart, k - 1L)
      fstart <- k + 1L
    }
    k <- k + 1L
  }
  bounds[[length(bounds) + 1L]] <- c(fstart, n)
  for (j in seq_along(bounds)) {
    b <- bounds[[j]]
    if (b[1] > b[2]) next
    if (!validUTF8(rawToChar(lr[b[1]:b[2]]))) return(j)
  }
  length(bounds)
}

# state machine: returns list of character vectors (fields per record)
split_csv_records <- function(text, path) {
  chars <- strsplit(text, "", fixed = TRUE)[[1]]
  n <- length(chars)
  recs <- list(); fields <- character(0); buf <- ""
  i <- 1L; in_q <- FALSE
  push_field <- function() { fields <<- c(fields, buf); buf <<- "" }
  while (i <= n) {
    ch <- chars[i]
    if (in_q) {
      if (ch == '"') {
        if (i < n && chars[i + 1L] == '"') { buf <- paste0(buf, '"'); i <- i + 2L; next }
        in_q <- FALSE; i <- i + 1L; next
      }
      if (ch == "\r") yamaa_error("invalid_csv", paste0("CR inside quoted field: ", path))
      buf <- paste0(buf, ch); i <- i + 1L; next
    }
    # not in quotes
    if (ch == '"') {
      if (nchar(buf) > 0) yamaa_error("invalid_csv", paste0("quote inside bare field: ", path))
      in_q <- TRUE; i <- i + 1L; next
    }
    if (ch == ",") { push_field(); i <- i + 1L; next }
    if (ch == "\r") {
      if (i < n && chars[i + 1L] == "\n") i <- i + 1L
      push_field(); recs[[length(recs) + 1]] <- fields; fields <- character(0); i <- i + 1L; next
    }
    if (ch == "\n") {
      push_field(); recs[[length(recs) + 1]] <- fields; fields <- character(0); i <- i + 1L; next
    }
    buf <- paste0(buf, ch); i <- i + 1L
  }
  if (in_q) yamaa_error("source_quote_unterminated", paste0("unterminated quoted field: ", path))
  if (nchar(buf) > 0 || length(fields) > 0) {
    push_field(); recs[[length(recs) + 1]] <- fields
  }
  recs
}

apply_column_type <- function(v, type, path, col) {
  if (type == "str") return(v)
  bad <- function(s) yamaa_error("field_parse_failed",
    paste0("column ", col, ": not ", type, " text: ", s))
  out <- switch(type,
    int = vapply(v, function(s) {
      if (is.na(s)) return(NA_integer_)
      tryCatch(to_int_value(parse_number_text(s)),
        error = function(e) bad(s))
    }, integer(1), USE.NAMES = FALSE),
    float = vapply(v, function(s) {
      if (is.na(s)) return(NA_real_)
      r <- tryCatch(parse_number_text(s), error = function(e) bad(s))
      if (!is.finite(r)) NA_real_ else r
    }, double(1), USE.NAMES = FALSE),
    date = vapply(v, function(s) {
      if (is.na(s)) return(NA_character_)
      tryCatch(parse_temporal_one(s, "date")$canon, error = function(e) bad(s))
    }, character(1), USE.NAMES = FALSE),
    datetime = vapply(v, function(s) {
      if (is.na(s)) return(NA_character_)
      tryCatch(parse_temporal_one(s, "datetime")$canon, error = function(e) bad(s))
    }, character(1), USE.NAMES = FALSE),
    yamaa_error("unknown_type", paste0("column type: ", type)))
  out
}

# ---- Parquet reading ------------------------------------------------------
# REQ-1032: the embedded Parquet type supplies the field type through this
# closed mapping (the inverse of REQ-0734). Anything else fails with
parquet_field_type <- function(tstr, field, path) {
  if (tstr %in% c("string", "large_string")) return("str")
  if (tstr == "int64") return("int")
  if (tstr == "double") return("float")
  if (tstr == "date32[day]") return("date")
  if (tstr == "timestamp[us]") return("datetime")
  yamaa_error("source_field_type_unsupported",
    paste0("field ", field, ": stored type ", tstr, ": ", path))
}

read_ydf_parquet <- function(path, types = list(), empty_string = "missing") {
  if (!requireNamespace("arrow", quietly = TRUE))
    yamaa_error("missing_dependency", "the arrow package is required for parquet input")
  tab <- tryCatch(arrow::read_parquet(path, as_data_frame = FALSE),
    error = function(e) yamaa_error("source_parquet_invalid",
      paste0("not a readable parquet file: ", path)))
  flds <- tab$schema$fields
  header <- vapply(flds, function(f) f$name, character(1))
  if (any(header == ""))
    yamaa_error("source_field_name_empty", paste0("empty field name: ", path))
  if (anyDuplicated(header))
    yamaa_error("source_field_name_duplicate", paste0("duplicate field name: ", path))
  # REQ-0532: types declaring a field not present in the source fail
  for (h in names(types)) {
    if (!h %in% header)
      yamaa_error("unknown_field", paste0("types declares unknown field: ", h))
  }
  # REQ-0533: the container supplies every field's type; a types entry for
  # a parquet field fails rather than overriding the source contract
  if (length(types) > 0)
    yamaa_error("redundant_field_type",
      paste0("types declared for parquet field: ", names(types)[1], ": ", path))
  tstr <- vapply(flds, function(f) f$type$ToString(), character(1))
  coltypes <- vapply(seq_along(flds), function(i)
    parquet_field_type(tstr[i], header[i], path), character(1), USE.NAMES = FALSE)
  names(coltypes) <- header
  cols <- lapply(seq_along(flds), function(i)
    parquet_read_column(tab[[header[i]]], coltypes[i], header[i], path, empty_string))
  df <- as.data.frame(cols, stringsAsFactors = FALSE, check.names = FALSE)
  names(df) <- header
  attr(df, "coltypes") <- coltypes
  df
}

# Read one parquet column into a storage vector. Records are 1-based data
# records (a parquet file has no header record).
parquet_read_column <- function(chunk, t, field, path, empty_string) {
  n <- chunk$length()
  if (t == "str") {
    v <- as.vector(chunk)
    v <- ifelse(is.na(v), NA_character_, as.character(v))
    # REQ-1159/1160: the empty-string convention applies at ingestion,
    # after the profile decodes; it touches str fields only
    if (empty_string == "missing") v[v == ""] <- NA_character_
    return(v)
  }
  if (t == "int") {
    v <- as.vector(chunk)
    return(vapply(v, function(x) {
      if (is.na(x)) return(NA_integer_)
      to_int_value(suppressWarnings(as.numeric(x)))
    }, integer(1), USE.NAMES = FALSE))
  }
  if (t == "float") {
    v <- as.vector(chunk)
    # REQ-1035: a non-finite DOUBLE normalizes to missing immediately
    v[!is.finite(v)] <- NA_real_
    return(v)
  }
  if (t == "date") {
    # REQ-0737's inverse: date32 days render as canonical date text
    s <- as.vector(chunk$cast(arrow::utf8()))
    return(vapply(seq_len(n), function(i) {
      x <- s[i]
      if (is.na(x)) return(NA_character_)
      # REQ-1041: the count must name 0001-01-01..9999-12-31
      if (x < "0001-01-01" || x > "9999-12-31")
        yamaa_error("source_field_value_invalid", paste0("field ", field,
          ", record ", i, ": date out of range: ", x, ": ", path))
      x
    }, character(1), USE.NAMES = FALSE))
  }
  if (t == "datetime") {
    # arrow renders timestamp[us] as "YYYY-MM-DD HH:MM:SS.ffffff"; the
    # cast is exact (no host floating point), so the wall clock survives
    s <- as.vector(chunk$cast(arrow::utf8()))
    return(vapply(seq_len(n), function(i) {
      x <- s[i]
      if (is.na(x)) return(NA_character_)
      res <- regmatches(x,
        regexec("^(\\d{4}-\\d{2}-\\d{2}) (\\d{2}:\\d{2}:\\d{2})\\.(\\d{6})$", x))
      m <- if (length(res)) res[[1]] else character(0)
      # REQ-1036/1041: whole seconds within the calendar range, no shifts
      if (length(m) == 0 || m[4] != "000000" ||
          m[2] < "0001-01-01" || m[2] > "9999-12-31")
        yamaa_error("source_field_value_invalid", paste0("field ", field,
          ", record ", i, ": not a whole-second calendar datetime: ", x, ": ", path))
      paste0(m[2], "T", m[3])
    }, character(1), USE.NAMES = FALSE))
  }
  yamaa_error("unknown_type", paste0("column type: ", t))
}

# ---- artifact writing -----------------------------------------------------
# write_typed_artifact(df, coltypes, path, decimals): the path's extension
# selects the profile (REQ-0716). df holds typed storage vectors.

write_typed_artifact <- function(df, coltypes, path, decimals = NULL) {
  prof <- profile_of_path(path)
  if (is.null(prof))
    yamaa_error("unknown_artifact_profile",
      paste0("no mapped artifact profile: ", path))
  if (prof == "csv") write_canonical_csv(render_csv_frame(df, coltypes, decimals), path)
  else write_yamaa_parquet(df, coltypes, path)
  invisible(path)
}

# REQ-0732: value text by column type. floats take the shortest round-trip
# text, or output.decimals' fixed-point form (csv only; REQ-0743 keeps
render_csv_frame <- function(df, coltypes, decimals) {
  out <- df
  for (h in names(df)) {
    t <- coltypes[[h]]; v <- df[[h]]
    out[[h]] <- switch(t,
      str = v,
      int = ifelse(is.na(v), NA_character_, as.character(v)),
      float = if (is.null(decimals)) float_text(v) else vapply(v,
        function(x) if (is.na(x)) NA_character_ else format_decimals(x, decimals),
        character(1), USE.NAMES = FALSE),
      date = v,
      datetime = v,
      yamaa_error("unknown_type", paste0("column type: ", t)))
  }
  out
}

# format a float with fixed decimals, round half away from zero (REQ-0747:
# exact scaling, never a host rounding routine -- see REQ-0748/0750)
format_decimals <- function(x, decimals) {
  mult <- 10^decimals
  # round half away from zero
  r <- sign(x) * floor(abs(x) * mult + 0.5) / mult
  # a value rounding to zero is written without a sign (REQ-0747)
  if (r == 0) r <- 0
  sprintf(paste0("%.", decimals, "f"), r)
}

# ---- canonical CSV output -------------------------------------------------
# REQ-0722..0733: LF terminators, UTF-8 no BOM, header row, missing -> empty
# bare field, collected empty string -> "", minimal quoting, floats shortest
# round-trip text without exponent, text never modified.

write_canonical_csv <- function(df, path) {
  header <- names(df)
  lines <- character(nrow(df) + 1L)
  lines[1] <- paste(vapply(header, csv_field, character(1)), collapse = ",")
  for (i in seq_len(nrow(df))) {
    row <- vapply(seq_along(header), function(j) {
      v <- df[[j]][i]
      if (is.na(v)) "" else csv_field(v)
    }, character(1))
    lines[i + 1L] <- paste(row, collapse = ",")
  }
  con <- file(path, "wb"); on.exit(close(con))
  writeLines(paste(lines, collapse = "\n"), con, useBytes = TRUE)
}

# REQ-0728: a field is quoted exactly when its text contains " , \r \n, or
# is the empty string. REQ-0731: missing is written bare-empty by the
csv_field <- function(s) {
  if (s == "" || grepl('[",\r\n]', s))
    paste0('"', gsub('"', '""', s, fixed = TRUE), '"')
  else s
}

# ---- parquet writing ------------------------------------------------------
# REQ-0734: each column type maps to exactly one physical/logical type.
# REQ-0735: fields are output.columns in order, all optional. REQ-0741:
# uncompressed pages. REQ-0743: floats enter as the binary64 the
# derivation produced; decimals never applies.

write_yamaa_parquet <- function(df, coltypes, path) {
  nms <- names(df)
  arrs <- lapply(nms, function(h) {
    tryCatch(parquet_array(df[[h]], coltypes[[h]]),
      yamaa_error = function(e) stop(e),
      error = function(e) yamaa_error("unwritable_value",
        paste0("column ", h, ": cannot write as ", coltypes[[h]],
          ": ", conditionMessage(e))))
  })
  tab <- do.call(arrow::arrow_table, setNames(arrs, nms))
  tryCatch(arrow::write_parquet(tab, path, compression = "uncompressed"),
    error = function(e) yamaa_error("publication_failed",
      paste0("parquet write failed: ", path, ": ", conditionMessage(e))))
  invisible(path)
}

parquet_array <- function(v, t) {
  if (t == "str")
    return(arrow::Array$create(v, type = arrow::utf8()))
  if (t == "int")
    return(arrow::Array$create(v, type = arrow::int64()))
  if (t == "float")
    return(arrow::Array$create(v, type = arrow::float64()))
  # REQ-0737/0739: the cast parses the canonical text exactly (whole
  # seconds); it attaches no zone, so the wall clock survives (REQ-0738)
  if (t == "date")
    return(arrow::Array$create(v)$cast(arrow::date32()))
  if (t == "datetime")
    return(arrow::Array$create(v)$cast(arrow::timestamp("us")))
  yamaa_error("unknown_type", paste0("column type: ", t))
}
