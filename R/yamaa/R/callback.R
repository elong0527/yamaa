# Keep secondary condition rendering from replacing a retained callback failure.
.function_detail <- function(read, fallback) {
  tryCatch(.scalar_text_bytes(read()), error = function(e) charToRaw(fallback),
           interrupt = function(e) charToRaw(fallback))
}
