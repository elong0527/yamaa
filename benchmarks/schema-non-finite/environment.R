numeric_constant <- function(kind) {
  constants <- list(
    "zero" = 0,
    "one" = 1,
    "positive-infinity" = Inf,
    "negative-infinity" = -Inf,
    "nan" = NaN
  )
  constants[[kind]]
}
