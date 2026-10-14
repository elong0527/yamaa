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


# Component truth follows exact integer/rational rounding of stored bits, not
# a host decimal formatter. In particular, 0x3f747ae147ae147b is above 1/200.
round_stored <- get(".project_decimal_round", asNamespace("yamaabenchmarks"))
from_bits <- function(bits) {
  positions <- seq.int(1L, 15L, by = 2L)
  readBin(as.raw(strtoi(substring(bits, positions, positions + 1L), 16L)),
          double(), 1L, size = 8L, endian = "big")
}
bits_of <- function(value) {
  paste0(as.character(writeBin(value, raw(), size = 8L, endian = "big")),
         collapse = "")
}
controls <- list(
  # Neighboring values lie on opposite sides of the decimal half at 0.005.
  list("3f747ae147ae147b", 2L, "3f847ae147ae147b"),
  list("3f747ae147ae147a", 2L, "0000000000000000"),
  list("bf747ae147ae147b", 2L, "bf847ae147ae147b"),
  # Exact .125/.375 ties choose the even decimal unit (.12/.38).
  list("3fc0000000000000", 2L, "3fbeb851eb851eb8"),
  list("3fd8000000000000", 2L, "3fd851eb851eb852"),
  list("3ff4000000000000", 1L, "3ff3333333333333"),
  # Stored 2.675 is strictly below 107/40; two decimals therefore yield 2.67.
  list("4005666666666666", 2L, "40055c28f5c28f5c"),
  list("8000000000000000", 2L, "8000000000000000"),
  list("bf70624dd2f1a9fc", 2L, "8000000000000000"),
  # Negative precision also chooses even units at the exact 250/350 halves.
  list("406f400000000000", -2L, "4069000000000000"),
  list("4075e00000000000", -2L, "4079000000000000"),
  # Min subnormal is below half a 1e-323 unit; twice it rounds to one unit,
  # whose nearest binary64 is exactly the original two-subnormal value.
  list("0000000000000001", 323L, "0000000000000000"),
  list("0000000000000002", 323L, "0000000000000002"),
  list("7fefffffffffffff", 2L, "7fefffffffffffff"),
  list("ffefffffffffffff", -309L, "8000000000000000")
)
for (control in controls) {
  stopifnot(identical(bits_of(round_stored(from_bits(control[[1L]]),
                                           control[[2L]])), control[[3L]]))
}
# Two 1e308 units overflow binary64; Python's finite round raises here too.
overflow <- tryCatch(round_stored(from_bits("7fefffffffffffff"), -308L),
                     error = identity)
stopifnot(inherits(overflow, "error"))
# These helper-level paths preserve the original percent and fine-precision
# controls, and the owner's warning-sensitive NA/nonfinite/adjust controls above.
stopifnot(identical(project_ratio(2, 400, 2L, 0, TRUE), 0.5))
stopifnot(identical(bits_of(project_ratio(from_bits("0000000000000001"),
                                        1, 324L, 0, FALSE)),
                    "0000000000000001"))
