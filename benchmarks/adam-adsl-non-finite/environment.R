r_numeric_constant <- function(kind) {
  switch(kind,
    "zero" = 0,
    "one" = 1,
    "positive-infinity" = Inf,
    "negative-infinity" = -Inf,
    "nan" = NaN,
    stop("unknown kind: ", kind))
}
