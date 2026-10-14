library(yamaabenchmarks)

# Independently authored binary and displayed expectations from the shared
# algorithm and the exact stored quotient (2 / 400 is just above 0.005).
stopifnot(identical(normal_cdf(-3), 0x1.61de1f985b5dcp-10))
stopifnot(identical(project_ratio(2, 400, 2L, 0, FALSE), 0.01))
stopifnot(identical(project_ratio(-2, 400, 2L, 0, FALSE), -0.01))
stopifnot(identical(project_ratio(25, 100, 1L, 0, FALSE), 0.2))
stopifnot(identical(project_ratio(5, 4, 1L, 0, FALSE), 1.2))
stopifnot(identical(project_ratio(1234, 1, -2L, 0, FALSE), 1200))

# Stored values on both sides of decimal ties, plus exact ties to even.
for (sign in c(-1, 1)) {
  stopifnot(identical(project_ratio(sign * 0x1.47ae147ae147ap-8, 1, 2L, 0, FALSE), 0))
  stopifnot(identical(project_ratio(sign * 0x1.47ae147ae147bp-8, 1, 2L, 0, FALSE), sign * 0.01))
  stopifnot(identical(project_ratio(sign * 2.675, 1, 2L, 0, FALSE), sign * 2.67))
  stopifnot(identical(project_ratio(sign * 1.75, 1, 1L, 0, FALSE), sign * 1.8))
  stopifnot(identical(project_ratio(sign * 50, 1, -2L, 0, FALSE), 0))
  stopifnot(identical(project_ratio(sign * 150, 1, -2L, 0, FALSE), sign * 200))
}
stopifnot(identical(project_ratio(2, 400, 2L, NA_real_, TRUE), 0.5))
stopifnot(identical(project_ratio(2, 400, 2L, 1.5, FALSE), 1.51))
stopifnot(identical(project_ratio(0x0.0000000000001p-1022, 1, 323L, 0, FALSE), 0))
stopifnot(identical(project_ratio(0x0.0000000000001p-1022, 1, 324L, 0, FALSE), 0x0.0000000000001p-1022))
stopifnot(identical(project_ratio(1e300, 1, -309L, 0, FALSE), 0))
