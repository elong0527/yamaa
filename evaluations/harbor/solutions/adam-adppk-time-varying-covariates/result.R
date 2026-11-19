# Reference solution for the yamaa benchmark adam-adppk-time-varying-covariates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

read_src <- function(name) {
  read_csv(file.path("/app/input", paste0(name, ".csv")),
           col_types = cols(.default = col_character()), na = "")
}

adsl <- read_src("adsl")
ex <- read_src("ex")
lb <- read_src("lb")
pc <- read_src("pc")
vs <- read_src("vs")

full_dtm <- function(x) {
  x <- ifelse(nchar(x) == 10, paste0(x, "T00:00"), x)
  ifelse(nchar(x) == 16, paste0(x, ":00"), x)
}

events <- bind_rows(
  ex %>% transmute(STUDYID, USUBJID, ADTM = full_dtm(EXSTDTC), EVID = 1L,
                   SRCSEQ = as.numeric(EXSEQ)),
  pc %>% transmute(STUDYID, USUBJID, ADTM = full_dtm(PCDTC), EVID = 0L,
                   SRCSEQ = as.numeric(PCSEQ))
) %>%
  filter(!is.na(ADTM)) %>%
  arrange(USUBJID, ADTM, EVID, SRCSEQ) %>%
  group_by(USUBJID) %>%
  mutate(ASEQ = row_number()) %>%
  ungroup() %>%
  mutate(ADT = substr(ADTM, 1, 10))

weights <- vs %>%
  filter(VSTESTCD == "WEIGHT") %>%
  transmute(USUBJID, DT = substr(VSDTC, 1, 10), DTC = VSDTC,
            SEQ = as.numeric(VSSEQ), VAL = as.numeric(VSSTRESN)) %>%
  filter(!is.na(VAL), !is.na(DT))

creats <- lb %>%
  filter(LBTESTCD == "CREAT") %>%
  transmute(USUBJID, DT = substr(LBDTC, 1, 10), DTC = LBDTC,
            SEQ = as.numeric(LBSEQ), VAL = as.numeric(LBSTRESN)) %>%
  filter(!is.na(VAL), !is.na(DT))

latest <- function(results, usubjid, adt) {
  vapply(seq_along(usubjid), function(i) {
    r <- results %>%
      filter(USUBJID == usubjid[i], DT <= adt[i]) %>%
      arrange(DT, DTC, SEQ)
    if (nrow(r) == 0) NA_real_ else r$VAL[nrow(r)]
  }, numeric(1))
}

adppk <- events %>%
  mutate(WT = latest(weights, USUBJID, ADT),
         CREAT = latest(creats, USUBJID, ADT)) %>%
  left_join(adsl %>% transmute(USUBJID, AGE = as.numeric(AGE), SEX), by = "USUBJID") %>%
  mutate(FACTOR = case_when(SEX == "M" ~ 1.23, SEX == "F" ~ 1.04, TRUE ~ NA_real_),
         CRCL = (140 - AGE) * WT * FACTOR / CREAT) %>%
  select(STUDYID, USUBJID, ASEQ, ADTM, EVID, WT, CREAT, CRCL)

write_csv(adppk, file.path("/app/output", "adppk.csv"), na = "")
