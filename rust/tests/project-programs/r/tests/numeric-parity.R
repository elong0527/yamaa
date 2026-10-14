library(yamaabenchmarks)

# Independently authored binary and displayed expectations from the shared
# algorithm and the exact stored quotient (2 / 400 is just above 0.005).
stopifnot(identical(normal_cdf(-3), 0x1.61de1f985b5dcp-10))
stopifnot(identical(project_ratio(2, 400, 2L, 0, FALSE), 0.01))
stopifnot(identical(project_ratio(-2, 400, 2L, 0, FALSE), -0.01))
stopifnot(identical(project_ratio(25, 100, 1L, 0, FALSE), 0.2))
stopifnot(identical(project_ratio(5, 4, 1L, 0, FALSE), 1.2))
stopifnot(identical(project_ratio(1234, 1, -2L, 0, FALSE), 1200))

# Exported helpers retain R's missing-precision behavior, and non-finite
# quotients must preserve their value without formatting/conversion warnings.
no_warning <- function(expr) {
  withCallingHandlers(expr, warning = function(w) stop(conditionMessage(w)))
}
stopifnot(identical(
  no_warning(project_ratio(2, 400, NA_integer_, 7, FALSE)),
  round(2 / 400, NA_integer_) + 7
))
stopifnot(identical(no_warning(project_ratio(2, 400, 2L, NA_real_, FALSE)), 0.01))
stopifnot(identical(no_warning(project_ratio(2, 400, 2L, 7, FALSE)), 7.01))
stopifnot(identical(no_warning(project_ratio(Inf, 1, 2L, -Inf, FALSE)), NaN))
for (value in list(NA_real_, NaN, Inf, -Inf)) {
  for (decimals in c(-2L, 2L, 324L, NA_integer_)) {
    stopifnot(identical(
      no_warning(project_ratio(value, 1, decimals, NA_real_, TRUE)),
      round(value * 100, decimals)
    ))
  }
}
