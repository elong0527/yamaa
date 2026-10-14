# Ordinary installed project calculations. Integer and text inputs retain the
# lossless SDK scalar classes until their explicit host conversion here.
bmi <- function(weight_kg, height_cm, cm_per_m = 100L) {
  weight_kg / (height_cm / as.integer(cm_per_m)) ^ 2
}
# Exact decimal rounding of a finite stored binary64 value. Integer limbs stay
# below 2^53 for every operation; no formatter or decimal parser is consulted.
.project_decimal_round <- function(value, digits) {
  base <- 2 ^ 26
  trim <- function(x) {
    while (length(x) > 1L && tail(x, 1L) == 0) x <- head(x, -1L)
    x
  }
  limbs <- function(x) {
    out <- numeric()
    repeat {
      out <- c(out, x %% base)
      x <- floor(x / base)
      if (x == 0) return(out)
    }
  }
  multiply <- function(x, factor) {
    carry <- 0
    for (i in seq_along(x)) {
      next_value <- x[i] * factor + carry
      x[i] <- next_value %% base
      carry <- floor(next_value / base)
    }
    if (carry != 0) x <- c(x, carry)
    trim(x)
  }
  shift <- function(x, count) {
    trim(c(rep(0, count %/% 26L), multiply(x, 2 ^ (count %% 26L))))
  }
  compare <- function(x, y) {
    if (length(x) != length(y)) return(sign(length(x) - length(y)))
    for (i in rev(seq_along(x))) if (x[i] != y[i]) return(sign(x[i] - y[i]))
    0
  }
  subtract <- function(x, y) {
    borrow <- 0
    for (i in seq_along(x)) {
      next_value <- x[i] - (if (i <= length(y)) y[i] else 0)
      next_value <- next_value - borrow
      borrow <- as.integer(next_value < 0)
      x[i] <- next_value + borrow * base
    }
    trim(x)
  }
  bits <- function(x) {
    high <- tail(x, 1L)
    count <- 0L
    while (high > 0) {
      count <- count + 1L
      high <- floor(high / 2)
    }
    (length(x) - 1L) * 26L + count
  }
  rounded_quotient <- function(x, y) {
    position <- bits(x) - bits(y)
    quotient <- 0
    if (position >= 0L) {
      for (i in seq.int(position, 0L)) {
        quotient <- multiply(quotient, 2)
        divisor <- shift(y, i)
        if (compare(x, divisor) >= 0) {
          x <- subtract(x, divisor)
          quotient[1L] <- quotient[1L] + 1
        }
      }
    }
    halfway <- compare(multiply(x, 2), y)
    if (halfway > 0 || (halfway == 0 && quotient[1L] %% 2 == 1)) {
      carry <- 1
      for (i in seq_along(quotient)) {
        next_value <- quotient[i] + carry
        quotient[i] <- next_value %% base
        carry <- floor(next_value / base)
        if (carry == 0) break
      }
      if (carry != 0) quotient <- c(quotient, carry)
    }
    trim(quotient)
  }
  power5 <- function(count) {
    result <- 1
    for (i in seq_len(count)) result <- multiply(result, 5)
    result
  }
  encode <- function(sign_bit, coefficient, exponent) {
    if (coefficient == 0) exponent <- -1023L
    if (exponent > 1023L) stop("rounded value too large to represent")
    fraction <- coefficient - (if (exponent >= -1022L) 2 ^ 52 else 0)
    field <- if (exponent >= -1022L) exponent + 1023L else 0L
    raw <- integer(8L)
    for (i in seq.int(8L, 3L)) {
      raw[i] <- fraction %% 256
      fraction <- floor(fraction / 256)
    }
    raw[2L] <- (field %% 16L) * 16L + fraction
    raw[1L] <- sign_bit + field %/% 16L
    readBin(as.raw(raw), double(), 1L, size = 8L, endian = "big")
  }

  raw <- as.integer(writeBin(as.double(value), raw(), size = 8L, endian = "big"))
  sign_bit <- (raw[1L] %/% 128L) * 128L
  field <- (raw[1L] %% 128L) * 16L + raw[2L] %/% 16L
  coefficient <- raw[2L] %% 16L
  for (i in 3L:8L) coefficient <- coefficient * 256 + raw[i]
  if (field != 0L) coefficient <- coefficient + 2 ^ 52
  exponent <- if (field == 0L) -1074L else field - 1075L
  if (coefficient == 0 || (digits >= 0L && exponent + digits >= 0L)) return(value)
  if (digits < -308L) return(encode(sign_bit, 0, -1023L))

  numerator <- limbs(coefficient)
  denominator <- 1
  if (digits >= 0L) {
    for (i in seq_len(digits)) numerator <- multiply(numerator, 5)
  } else {
    denominator <- power5(-digits)
  }
  binary_shift <- exponent + digits
  if (binary_shift >= 0L) {
    numerator <- shift(numerator, binary_shift)
  } else {
    denominator <- shift(denominator, -binary_shift)
  }
  units <- rounded_quotient(numerator, denominator)
  if (identical(units, 0)) return(encode(sign_bit, 0, -1023L))

  numerator <- units
  denominator <- 1
  if (digits >= 0L) {
    denominator <- shift(power5(digits), digits)
  } else {
    for (i in seq_len(-digits)) numerator <- multiply(numerator, 5)
    numerator <- shift(numerator, -digits)
  }
  exponent <- bits(numerator) - bits(denominator)
  ordering <- if (exponent >= 0L) {
    compare(numerator, shift(denominator, exponent))
  } else {
    compare(shift(numerator, -exponent), denominator)
  }
  if (ordering < 0) exponent <- exponent - 1L
  binary_shift <- max(exponent - 52L, -1074L)
  if (binary_shift >= 0L) {
    denominator <- shift(denominator, binary_shift)
  } else {
    numerator <- shift(numerator, -binary_shift)
  }
  rounded <- rounded_quotient(numerator, denominator)
  coefficient <- sum(rounded * base ^ (seq_along(rounded) - 1L))
  if (coefficient >= 2 ^ 53) {
    coefficient <- coefficient / 2
    binary_shift <- binary_shift + 1L
  }
  exponent <- if (coefficient < 2 ^ 52) -1023L else binary_shift + 52L
  encode(sign_bit, coefficient, exponent)
}

project_ratio <- function(numerator, denominator, decimals = 2L,
                          adjust = 0, as_percent = FALSE) {
  value <- numerator / denominator
  if (as_percent) value <- value * 100
  decimals <- as.integer(decimals)
  # Decimal units and the returned binary64 are rounded from exact stored
  # bytes, independently of platform printf or approximate decimal ties.
  value <- if (is.na(decimals)) {
    round(value, decimals)
  } else if (!is.finite(value) || decimals > 323L) {
    # Non-finite values and finer precision need no decimal conversion.
    value
  } else {
    .project_decimal_round(value, decimals)
  }
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
