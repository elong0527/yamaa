# Ordinary installed project calculations over plain R values: an int
# argument arrives as a double and a str argument as character.
bmi <- function(weight_kg, height_cm, cm_per_m = 100L) {
  weight_kg / (height_cm / as.integer(cm_per_m)) ^ 2
}
# Round the exact stored binary64 value to decimal places with ties to even,
# as Python round() does. Windows sprintf() and R round() can treat a value
# just above a decimal tie (such as 2 / 400) as the tie itself.
.project_round <- function(value, decimals) {
  if (!is.finite(value) || value == 0 || decimals > 323L) return(value)
  if (decimals < -308L) return(value * 0)

  # Little-endian decimal digits keep every operation small and exact.
  multiply <- function(digits, factor, carry = 0L) {
    for (index in seq_along(digits)) {
      product <- digits[[index]] * factor + carry
      digits[[index]] <- product %% 10L
      carry <- product %/% 10L
    }
    if (carry) digits <- c(digits, carry)
    digits
  }
  bytes <- as.integer(writeBin(abs(value), raw(), size = 8L, endian = "big"))
  exponent <- (bytes[[1L]] %% 128L) * 16L + bytes[[2L]] %/% 16L
  digits <- as.integer(exponent != 0L)
  for (index in 2:8) {
    for (bit in if (index == 2L) 3:0 else 7:0) {
      digits <- multiply(digits, 2L, bitwAnd(bitwShiftR(bytes[[index]], bit), 1L))
    }
  }
  shift <- max(exponent, 1L) - 1023L - 52L
  scale <- max(-shift, 0L)
  for (step in seq_len(abs(shift))) {
    digits <- multiply(digits, if (shift < 0L) 5L else 2L)
  }

  discard <- scale - decimals
  if (discard <= 0L) return(value)
  if (length(digits) <= discard) {
    digits <- c(digits, integer(discard + 1L - length(digits)))
  }
  remainder <- digits[seq_len(discard)]
  kept <- digits[-seq_len(discard)]
  halfway <- remainder[[discard]]
  if (halfway > 5L || (halfway == 5L &&
      (any(remainder[-discard] != 0L) || kept[[1L]] %% 2L == 1L))) {
    kept <- multiply(kept, 1L, 1L)
  }
  rounded <- as.numeric(paste0(if (value < 0) "-" else "",
    paste0(rev(kept), collapse = ""), "e", -decimals))
  if (!is.finite(rounded)) stop("rounded value is too large")
  rounded
}

project_ratio <- function(numerator, denominator, decimals = 2L,
                          adjust = 0, as_percent = FALSE) {
  value <- numerator / denominator
  if (as_percent) value <- value * 100
  value <- .project_round(value, as.integer(decimals))
  if (is.na(adjust)) adjust <- 0
  value + adjust
}
numeric_constant <- function(kind) {
  values <- list(zero = 0, one = 1, `positive-infinity` = Inf,
                 `negative-infinity` = -Inf, nan = NaN)
  values[[as.character(kind)]]
}
project_value <- function(x) x
# A dose in mg per kg of body weight; a weight that is not positive is refused,
# as the Python project function refuses it.
dose_per_kg <- function(dose_mg, weight_kg) {
  if (weight_kg <= 0) stop("weight_kg must be positive")
  dose_mg / weight_kg
}

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
  # R's decimal literal parser can differ from Python by several ulps.
  tiny <- 0x1.56e1fc2f8f359p-997
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
