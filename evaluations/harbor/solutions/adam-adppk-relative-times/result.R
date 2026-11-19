# Reference solution for the yamaa benchmark adam-adppk-relative-times (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(lubridate)
})

parse_dtc <- function(x) {
  parse_date_time(x, orders = c("Ymd HMS", "Ymd HM", "Ymd H", "Ymd"), tz = "UTC", quiet = TRUE)
}

format_dtm <- function(x) {
  ifelse(is.na(x), NA_character_, format(x, "%Y-%m-%dT%H:%M:%S", tz = "UTC"))
}

hours_between <- function(from, to) {
  as.numeric(difftime(to, from, units = "secs")) / 3600
}

ex <- read_csv("/app/input/ex.csv", col_types = cols(.default = col_character()))
pc <- read_csv("/app/input/pc.csv", col_types = cols(.default = col_character()))

doses <- ex %>%
  mutate(EXDOSE = as.numeric(EXDOSE), EXSEQ = as.numeric(EXSEQ)) %>%
  filter(!is.na(EXDOSE), EXDOSE > 0) %>%
  transmute(STUDYID, USUBJID, SRCSEQ = EXSEQ, ADTM = parse_dtc(EXSTDTC), EVID = 1, AMT = EXDOSE)

samples <- pc %>%
  transmute(STUDYID, USUBJID, SRCSEQ = as.numeric(PCSEQ), ADTM = parse_dtc(PCDTC), EVID = 0, AMT = NA_real_)

first_dose <- doses %>%
  filter(!is.na(ADTM)) %>%
  arrange(USUBJID, ADTM, SRCSEQ) %>%
  group_by(USUBJID) %>%
  summarise(FANLDTM = first(ADTM), FDOSE = first(AMT), .groups = "drop")

previous_dose <- function(usubjid, adtm) {
  d <- doses %>%
    filter(USUBJID == usubjid, !is.na(ADTM), ADTM < adtm) %>%
    arrange(ADTM, SRCSEQ)
  if (is.na(adtm) || nrow(d) == 0) {
    return(tibble(PDTM = as.POSIXct(NA, tz = "UTC"), PDOSE = NA_real_))
  }
  tibble(PDTM = last(d$ADTM), PDOSE = last(d$AMT))
}

prev <- bind_rows(Map(previous_dose, samples$USUBJID, samples$ADTM))
samples <- bind_cols(samples, prev)

adppk <- bind_rows(doses, samples) %>%
  left_join(first_dose, by = "USUBJID") %>%
  arrange(USUBJID, ADTM, EVID, SRCSEQ) %>%
  group_by(USUBJID) %>%
  mutate(ASEQ = row_number()) %>%
  ungroup() %>%
  mutate(
    AFRLT = hours_between(FANLDTM, ADTM),
    APRLT = case_when(
      EVID == 1 ~ 0,
      !is.na(PDTM) ~ hours_between(PDTM, ADTM),
      TRUE ~ AFRLT
    ),
    DOSEA = case_when(
      EVID == 1 ~ AMT,
      !is.na(PDTM) ~ PDOSE,
      TRUE ~ FDOSE
    ),
    ADTM = format_dtm(ADTM),
    FANLDTM = format_dtm(FANLDTM)
  ) %>%
  select(STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, FANLDTM, AFRLT, APRLT, DOSEA)

write_csv(adppk, "/app/output/adppk.csv", na = "")
