# Declared R text crosses to Rust only as raw UTF-8 bytes; Rust never borrows
# an unchecked R string. There is no locale fallback or Unicode repair.
.utf8_raw <- function(x) {
  if (!is.character(x) || length(x) != 1L || is.na(x) || !is.null(attributes(x))) {
    stop("expected one unclassed non-missing character scalar", call. = FALSE)
  }
  encoding <- Encoding(x)
  if (identical(encoding, "bytes")) {
    stop("byte-marked R text has no declared Unicode encoding", call. = FALSE)
  }
  # A declared Latin-1 character has one defined Unicode decoding. Unmarked or
  # UTF-8 text is passed as held bytes for strict UTF-8 validation, never repaired.
  if (identical(encoding, "latin1")) x <- enc2utf8(x)
  charToRaw(x)
}

# Keep secondary condition rendering from replacing a retained callback failure.
.function_detail <- function(read, fallback) {
  tryCatch(.utf8_raw(read()), error = function(e) charToRaw(fallback),
           interrupt = function(e) charToRaw(fallback))
}

# Formatting is explicitly limited to ordinary host conditions; interrupts stay
# rooted in the native attempt and propagate only after Rust returns.
.project_condition_details <- function(condition) {
  list(.function_detail(function() class(condition)[1L],"R condition"),
       .function_detail(function() conditionMessage(condition),"unavailable R condition message"))
}
