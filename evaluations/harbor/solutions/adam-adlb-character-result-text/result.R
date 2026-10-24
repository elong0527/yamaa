# Reference solution for the yamaa benchmark adam-adlb-character-result-text (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(AVAL = col_double(), .default = col_character())
)

# The value rounded to `places` decimals with an exact half going away from
# zero; a value stored a hair below a half counts as the half it stands for.
# A result of zero is written without a minus sign.
display <- function(value, places) {
  scaled <- abs(value) * 10^places
  whole <- floor(scaled)
  whole <- ifelse(scaled - whole >= 0.5 - 1e-9, whole + 1, whole)
  rounded <- sign(value) * whole / 10^places
  rounded[!is.na(rounded) & rounded == 0] <- 0
  ifelse(is.na(value), NA_character_, formatC(rounded, format = "f", digits = places))
}

two <- display(lb$AVAL, 2)
adlb <- data.frame(
  STUDYID = lb$STUDYID,
  USUBJID = lb$USUBJID,
  PARAMCD = lb$PARAMCD,
  AVAL = lb$AVAL,
  # Right-aligned in eight characters; a wider text is written in full.
  AVALC = ifelse(is.na(two), "NOT DONE", sprintf("%8s", two)),
  AVALCS = display(lb$AVAL, 1)
)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
