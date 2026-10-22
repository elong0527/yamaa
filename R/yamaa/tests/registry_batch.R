library(yamaa)
# Unlocked synthetic registry under R: caller-supplied bindings and batch
# invocation against independently authored expected results. No renv
# lock is requested anywhere in this script.
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
reject <- function(expr, pattern = NULL) {
  error <- tryCatch(force(expr), error = identity)
  stopifnot_case(if (is.null(pattern)) "a refusal" else pattern, "the call is refused" = inherits(error, "error"))
  if (!is.null(pattern) && !grepl(pattern, conditionMessage(error), fixed = TRUE))
    fail_expectation("the refusal names its cause", conditionMessage(error), pattern)
  invisible(error)
}
add <- function(x, y) x + y
registry <- list("synthetic::add" = add)
expect_identical(yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::add", c("x", "y")), add,
  "the registry resolves the caller's binding")
reject(
  yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::absent", c("x")),
  "registered binding is missing"
)
reject(
  yamaa:::.yamaa_resolve_registry_function(registry, "synthetic::add", "x"),
  "signature differs"
)
reject(
  yamaa:::.yamaa_resolve_registry_function(
    list("synthetic::add" = add, "synthetic::add" = add), "synthetic::add", c("x", "y")),
  "duplicated"
)
vector_add <- function(x, y) x + ifelse(is.na(y), 0, y)
# Authored truth: NA is the accepting-missing null and counts as 0. Python's
# batch control adds a row at the signed 64-bit maximum; R exchanges int as a
# plain double, which holds integers exactly only within 2^53, so its last row
# is that boundary instead.
columns <- list(x = c(1, 3, 4, 2^53 - 1), y = c(2, 10, NA, 1))
expected <- c(3, 13, 4, 9007199254740992)
expect_identical(yamaa:::.yamaa_call_batch(vector_add, columns), expected, "the batch gives the authored results")
expect_identical(mapply(function(x, y) if (is.na(y)) x else x + y, columns$x, columns$y, USE.NAMES = FALSE),
  expected, "the batch equals the scalar calls")
reject(yamaa:::.yamaa_call_batch(vector_add, list(x = 1, y = c(1, 2))),
       "differ in length")
reject(yamaa:::.yamaa_call_batch(function(x, y) 1, list(x = 1:2, y = 1:2)),
       "wrong length")
expect_identical(yamaa:::.yamaa_call_batch(vector_add, list(x = numeric(0), y = numeric(0))), numeric(0),
  "an empty batch gives no results")
