# JSON objects ignore key order; arrays and scalar values retain their exact form.
context_equal <- function(actual, expected) {
  ordered <- function(value) {
    if (!is.list(value)) return(value)
    if (!is.null(names(value))) value <- value[order(names(value), method = "radix")]
    lapply(value, ordered)
  }
  identical(ordered(actual), ordered(expected))
}
