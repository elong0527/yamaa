# Reference solution for the yamaa benchmark sdtm-vs-units (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

fmt_text <- function(x) {
  if (is.na(x)) {
    return(NA_character_)
  }
  if (x == floor(x)) {
    formatC(x, format = "f", digits = 0)
  } else {
    format(x, scientific = FALSE, trim = TRUE, digits = 15)
  }
}

fmt_orres <- function(x) {
  if (is.na(x) || x == "") {
    return(NA_character_)
  }
  num <- suppressWarnings(as.numeric(x))
  if (is.na(num)) {
    return(x)
  }
  fmt_text(num)
}

raw <- read_csv(
  "/app/input/vs_raw.csv",
  col_types = cols(VSSEQ = col_integer(), .default = col_character())
)

vs <- raw |>
  mutate(
    has_result = !is.na(VSORRES) & VSORRES != "",
    VSORRES = vapply(VSORRES, fmt_orres, character(1)),
    value = suppressWarnings(as.numeric(raw$VSORRES)),
    VSSTRESN = case_when(
      !has_result ~ NA_real_,
      VSTESTCD == "HEIGHT" ~ value,
      VSTESTCD == "WEIGHT" & VSORRESU == "LB" ~ value * 0.45359237,
      VSTESTCD == "WEIGHT" ~ value,
      VSORRESU == "F" ~ (value - 32) * 5 / 9,
      .default = value
    ),
    VSSTRESC = vapply(VSSTRESN, fmt_text, character(1)),
    VSSTRESU = case_when(
      !has_result ~ NA_character_,
      VSTESTCD == "HEIGHT" ~ "cm",
      VSTESTCD == "WEIGHT" ~ "kg",
      .default = "C"
    ),
    VSORRESU = if_else(has_result, VSORRESU, NA_character_),
    VSSTAT = if_else(has_result, NA_character_, "NOT DONE"),
    DOMAIN = "VS",
    test_ord = case_when(
      VSTESTCD == "HEIGHT" ~ 1L,
      VSTESTCD == "WEIGHT" ~ 2L,
      .default = 3L
    )
  ) |>
  arrange(test_ord, USUBJID, VSSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES,
    VSORRESU, VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
