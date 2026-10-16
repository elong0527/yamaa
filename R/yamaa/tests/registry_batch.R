library(yamaa)
# Unlocked synthetic registry under R: caller-supplied bindings and batch
# invocation against independently authored expected results. No renv
# lock is requested anywhere in this script.
reject <- function(expr, pattern = NULL) {
  error <- tryCatch(force(expr), error = identity)
  stopifnot(inherits(error, "error"))
  if (!is.null(pattern)) stopifnot(grepl(pattern, conditionMessage(error), fixed = TRUE))
  invisible(error)
}
add <- function(x, y) x + y
registry <- list("synthetic::add" = add)
stopifnot(identical(
  yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::add", c("x", "y")),
  add
))
reject(
  yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::absent", c("x")),
  "registered binding is missing"
)
reject(
  yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::add", "x"),
  "signature differs"
)
vector_add <- function(x, y) x + ifelse(is.na(y), 0, y)
# Authored truth: NA is the accepting-missing null and counts as 0.
columns <- list(x = c(1, 3, 4), y = c(2, 10, NA))
stopifnot(identical(yamaa:::.yamaa_call_batch(vector_add, columns), c(3, 13, 4)))
stopifnot(identical(
  mapply(function(x, y) if (is.na(y)) x else x + y, columns$x, columns$y,
         USE.NAMES = FALSE),
  c(3, 13, 4)
))
reject(yamaa:::.yamaa_call_batch(vector_add, list(x = 1, y = c(1, 2))),
       "differ in length")
reject(yamaa:::.yamaa_call_batch(function(x, y) 1, list(x = 1:2, y = 1:2)),
       "wrong length")
stopifnot(identical(yamaa:::.yamaa_call_batch(vector_add, list(x = numeric(0), y = numeric(0))),
                    numeric(0)))
