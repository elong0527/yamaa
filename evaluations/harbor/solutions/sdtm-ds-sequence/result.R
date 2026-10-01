# Reference solution for the yamaa benchmark sdtm-ds-sequence (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

ds_raw <- read_csv(
  "/app/input/ds_raw.csv",
  col_types = cols(.default = col_character())
)

complete_date <- function(x) {
  x <- str_trim(x)
  if (is.na(x) || x == "") {
    return(as.Date(NA))
  }
  parts <- str_split(x, "-", simplify = TRUE)
  n <- ncol(parts)
  if (n == 3 && nchar(parts[1]) == 4) {
    return(suppressWarnings(as.Date(x)))
  }
  if (n == 2 && nchar(parts[1]) == 4) {
    y <- suppressWarnings(as.integer(parts[1]))
    m <- suppressWarnings(as.integer(parts[2]))
    if (is.na(y) || is.na(m) || m < 1 || m > 12) {
      return(as.Date(NA))
    }
    return(suppressWarnings(as.Date(sprintf("%04d-%02d-15", y, m))))
  }
  if (n == 1 && nchar(x) == 4) {
    y <- suppressWarnings(as.integer(x))
    if (is.na(y)) {
      return(as.Date(NA))
    }
    return(suppressWarnings(as.Date(sprintf("%04d-06-15", y))))
  }
  as.Date(NA)
}

ds <- ds_raw |>
  rowwise() |>
  mutate(
    DOMAIN = "DS",
    STUDYID = STUDY,
    USUBJID = PATNUM,
    DSCAT = ifelse(DSDECOD == "RANDOMIZED", "PROTOCOL MILESTONE", "DISPOSITION EVENT"),
    DSDTC = complete_date(DSDTCOL)
  ) |>
  ungroup() |>
  arrange(USUBJID, DSDTC, DSDECOD) |>
  group_by(USUBJID) |>
  mutate(DSSEQ = row_number()) |>
  ungroup() |>
  select(DOMAIN, STUDYID, USUBJID, DSSEQ, DSDECOD, DSCAT, DSDTC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ds, "/app/output/ds.csv", na = "")
