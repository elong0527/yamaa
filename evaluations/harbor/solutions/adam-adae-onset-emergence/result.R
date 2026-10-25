# Reference solution for the yamaa benchmark adam-adae-onset-emergence (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

# ADAE: treatment emergence decided at the second, one record per AE record.
suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(stringr)
})

in_dir <- "/app/input"
out_dir <- "/app/output"
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)

read_chr <- function(path) {
  read_csv(path, col_types = cols(.default = col_character()),
           na = c(""), trim_ws = TRUE, progress = FALSE)
}

adsl <- read_chr(file.path(in_dir, "adsl.csv"))
ae <- read_chr(file.path(in_dir, "ae.csv"))

# ISO 8601 datetime text to POSIXct (UTC, so a day is always 24 hours).
# A value without seconds falls on second zero; anything else is no value.
parse_dtm <- function(x) {
  x <- str_trim(x)
  full <- case_when(
    is.na(x) ~ NA_character_,
    str_detect(x, "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}$") ~ x,
    str_detect(x, "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}$") ~ paste0(x, ":00"),
    TRUE ~ NA_character_
  )
  as.POSIXct(full, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC")
}

fmt_dtm <- function(x) {
  out <- format(x, "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  out[is.na(x)] <- NA_character_
  out
}

day_secs <- 24 * 60 * 60

adsl_t <- adsl %>%
  transmute(
    STUDYID, USUBJID, EOTSTT,
    TRTSDTM = parse_dtm(TRTSDTM),
    TRTEDTM = parse_dtm(TRTEDTM)
  )

adae <- ae %>%
  mutate(AESEQ = as.integer(AESEQ), .row = row_number()) %>%
  left_join(adsl_t, by = c("STUDYID", "USUBJID")) %>%
  mutate(
    ASTDTM = parse_dtm(AESTDTC),
    treated = !is.na(TRTSDTM),
    ongoing = !is.na(EOTSTT) & EOTSTT == "ONGOING",
    window_days = if_else(!is.na(AESER) & AESER == "N", 30, 90),
    TEENDDTM = if_else(treated & !ongoing,
                       TRTEDTM + window_days * day_secs,
                       as.POSIXct(NA, tz = "UTC")),
    TRTEMFL = case_when(
      !treated ~ NA_character_,
      is.na(ASTDTM) ~ "Y",
      ongoing & ASTDTM >= TRTSDTM ~ "Y",
      !ongoing & !is.na(TEENDDTM) & ASTDTM >= TRTSDTM & ASTDTM <= TEENDDTM ~ "Y",
      TRUE ~ NA_character_
    )
  )

# AOCCFL: earliest treatment-emergent event with an ASTDTM, lower AESEQ on ties.
first_te <- adae %>%
  filter(TRTEMFL == "Y", !is.na(ASTDTM)) %>%
  arrange(USUBJID, ASTDTM, AESEQ) %>%
  group_by(USUBJID) %>%
  slice(1) %>%
  ungroup() %>%
  select(.row)

adae <- adae %>%
  mutate(AOCCFL = if_else(.row %in% first_te$.row, "Y", NA_character_)) %>%
  arrange(.row) %>%
  transmute(
    STUDYID, USUBJID, AESEQ, AETERM, AESER,
    ASTDTM = fmt_dtm(ASTDTM),
    TRTSDTM = fmt_dtm(TRTSDTM),
    TRTEDTM = fmt_dtm(TRTEDTM),
    EOTSTT,
    TEENDDTM = fmt_dtm(TEENDDTM),
    TRTEMFL, AOCCFL
  )

write_csv(adae, file.path(out_dir, "adae.csv"), na = "")
