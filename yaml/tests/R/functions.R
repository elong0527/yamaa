# Host functions reachable from the `call` operation, R implementation.
#
# Every entry must match tests/python/functions.py in name, arguments and
# result, or a specification using `call` is not portable.

fn_study_day <- function(date_, reference) {
  d <- suppressWarnings(as.Date(substr(as.character(date_), 1, 10)))
  r <- suppressWarnings(as.Date(substr(as.character(reference), 1, 10)))
  if (is_missing(date_) || is_missing(reference) || is.na(d) || is.na(r)) return(NULL)
  diff <- as.integer(d - r)
  if (diff >= 0L) diff + 1L else diff
}

fn_concat <- function(values, sep = "") {
  if (any(vapply(values, is_missing, logical(1)))) return(NULL)
  paste(vapply(values, as.character, ""), collapse = as.character(sep))
}

HOST_FUNCTIONS <- list(study_day = fn_study_day, concat = fn_concat)
