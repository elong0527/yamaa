# Reference solution for the yamaa benchmark adam-adsl-treatment (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)
library(lubridate)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXSEQ = col_integer(), .default = col_character())
)

qual <- c("VITAMIN D3", "PLACEBO")

parse_start <- function(x) {
  x[!is.na(x) & x == ""] <- NA_character_
  dt <- suppressWarnings(ymd_hms(x, quiet = TRUE, tz = "UTC"))
  d <- suppressWarnings(ymd(x, quiet = TRUE, tz = "UTC"))
  coalesce(dt, as.POSIXct(d, tz = "UTC"))
}

parse_end <- function(x) {
  base <- parse_start(x)
  is_date <- !is.na(x) & x != "" & !str_detect(x, "T")
  base[is_date] <- base[is_date] + 23 * 3600 + 59 * 60 + 59
  base
}

ex_dt <- ex |>
  mutate(
    `__start_dt` = parse_start(EXSTDTC),
    `__end_dt` = parse_end(EXENDTC)
  )

first_start <- ex_dt |>
  filter(EXTRT %in% qual, !is.na(`__start_dt`)) |>
  arrange(`__start_dt`, EXSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, TRT01RAW = EXTRT, `__stdtm` = `__start_dt`, `__stdtc` = EXSTDTC)

last_end <- ex_dt |>
  filter(EXTRT %in% qual, !is.na(`__end_dt`)) |>
  arrange(desc(`__end_dt`)) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, `__endtm` = `__end_dt`, `__endtc` = EXENDTC)

adsl <- dm |>
  left_join(first_start, by = c("STUDYID", "USUBJID")) |>
  left_join(last_end, by = c("STUDYID", "USUBJID")) |>
  mutate(
    `__src` = coalesce(TRT01RAW, ACTARM),
    TRT01A = case_when(
      is.na(`__src`) | `__src` == "" ~ "NOT TREATED",
      .default = str_to_upper(`__src`)
    ),
    TRTSDTM = format(`__stdtm`, "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
    TRTSDT = as.Date(`__stdtm`, tz = "UTC"),
    TRTSTMF = if_else(!is.na(`__stdtc`) & !str_detect(`__stdtc`, "T"), "H", NA_character_),
    TRTEDTM = format(`__endtm`, "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
    TRTEDT = as.Date(`__endtm`, tz = "UTC"),
    TRTETMF = if_else(!is.na(`__endtc`) & !str_detect(`__endtc`, "T"), "H", NA_character_),
    TRTDURD = as.integer(TRTEDT - TRTSDT) + 1L,
    SAFFL = if_else(is.na(TRTSDT), "N", "Y")
  ) |>
  select(
    STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF,
    TRTEDT, TRTEDTM, TRTETMF, TRTDURD, SAFFL
  ) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
