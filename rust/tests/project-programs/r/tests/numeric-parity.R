library(yamaabenchmarks)

# Independently authored binary and displayed expectations from the shared
# algorithm and the exact stored quotient (2 / 400 is just above 0.005).
stopifnot(identical(normal_cdf(-3), 0x1.61de1f985b5dcp-10))
stopifnot(identical(project_ratio(2, 400, 2L, 0, FALSE), 0.01))
stopifnot(identical(project_ratio(-2, 400, 2L, 0, FALSE), -0.01))
stopifnot(identical(project_ratio(25, 100, 1L, 0, FALSE), 0.2))
stopifnot(identical(project_ratio(5, 4, 1L, 0, FALSE), 1.2))
stopifnot(identical(project_ratio(1234, 1, -2L, 0, FALSE), 1200))
