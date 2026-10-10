# Ordinary installed project calculations. Integer and text inputs retain the
# lossless SDK scalar classes until their explicit host conversion here.
bmi <- function(weight_kg, height_cm, cm_per_m = 100L) {
  weight_kg / (height_cm / as.integer(cm_per_m)) ^ 2
}
project_ratio <- function(numerator, denominator, decimals = 2L,
                          adjust = 0, as_percent = FALSE) {
  value <- numerator / denominator
  if (as_percent) value <- value * 100
  value <- round(value, as.integer(decimals))
  if (is.na(adjust)) adjust <- 0
  value + adjust
}
numeric_constant <- function(kind) {
  values <- list(zero = 0, one = 1, `positive-infinity` = Inf,
                 `negative-infinity` = -Inf, nan = NaN)
  values[[as.character(kind)]]
}
project_value <- function(x) x

# The same authored algorithm and constants as the Python project function.
.project_exp <- function(x) {
  ln2 <- 0.6931471805599453
  doubling <- trunc(x / ln2 + if (x >= 0) 0.5 else -0.5)
  remainder <- x - doubling * ln2
  term <- total <- 1
  step <- 0
  repeat {
    step <- step + 1
    term <- term * (remainder / step)
    total <- total + term
    if (abs(term) <= 1e-18 * abs(total)) break
  }
  total * 2 ^ doubling
}
.erf_series <- function(z) {
  square <- z * z
  power <- total <- z
  step <- 0
  repeat {
    step <- step + 1
    power <- power * (-square / step)
    contribution <- power / (2 * step + 1)
    total <- total + contribution
    if (abs(contribution) <= 1e-18 * abs(total)) break
  }
  1.1283791670955126 * total
}
.erfc_tail <- function(z) {
  tiny <- 1e-300
  fraction <- numerator <- tiny
  denominator <- 0
  step <- 1
  while (step < 300) {
    partial <- if (step == 1) 1 else (step - 1) / 2
    denominator <- z + partial * denominator
    if (denominator == 0) denominator <- tiny
    numerator <- z + partial / numerator
    if (numerator == 0) numerator <- tiny
    denominator <- 1 / denominator
    factor <- numerator * denominator
    fraction <- fraction * factor
    if (abs(factor - 1) < 1e-17) break
    step <- step + 1
  }
  0.5641895835477563 * .project_exp(-z * z) * fraction
}
.erfc <- function(z) {
  if (z < -2) return(2 - .erfc_tail(-z))
  if (z > 2) return(.erfc_tail(z))
  1 - .erf_series(z)
}
normal_cdf <- function(q) 0.5 * .erfc(-q / 1.4142135623730951)
