# Reference solution for the yamaa benchmark adam-adppk-blq-methods (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(stringr)
})

ex <- read_csv("/app/input/ex.csv", col_types = cols(.default = col_character()))
pc <- read_csv("/app/input/pc.csv", col_types = cols(.default = col_character()))

with_seconds <- function(x) {
  x <- str_trim(x)
  case_when(
    str_detect(x, "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}$") ~ str_c(x, ":00"),
    str_detect(x, "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}") ~ str_sub(x, 1, 19),
    TRUE ~ x
  )
}

doses <- ex %>%
  transmute(
    STUDYID, USUBJID,
    ADTM = with_seconds(EXSTDTC),
    EVID = 1L,
    AMT = as.numeric(EXDOSE),
    ALLOQ = NA_real_,
    CENS = 0L,
    DV = NA_real_,
    DVM6 = NA_real_,
    SRCSEQ = as.numeric(EXSEQ)
  )

samples <- pc %>%
  filter(is.na(PCSTAT) | str_to_upper(str_trim(PCSTAT)) != "NOT DONE") %>%
  mutate(
    ADTM = with_seconds(PCDTC),
    RESN = as.numeric(PCSTRESN),
    ALLOQ = as.numeric(PCLLOQ),
    BLQ = is.na(RESN) & !is.na(PCSTRESC) & str_starts(str_trim(PCSTRESC), fixed("<")),
    SRCSEQ = as.numeric(PCSEQ)
  ) %>%
  arrange(USUBJID, ADTM, SRCSEQ) %>%
  group_by(USUBJID) %>%
  mutate(PREVQUANT = lag(!BLQ & !is.na(RESN), default = FALSE)) %>%
  ungroup() %>%
  transmute(
    STUDYID, USUBJID, ADTM,
    EVID = 0L,
    AMT = NA_real_,
    ALLOQ,
    CENS = if_else(BLQ, 1L, 0L),
    DV = if_else(BLQ, ALLOQ, RESN),
    DVM6 = case_when(
      !BLQ ~ RESN,
      PREVQUANT ~ ALLOQ / 2,
      TRUE ~ NA_real_
    ),
    SRCSEQ
  )

adppk <- bind_rows(samples, doses) %>%
  arrange(USUBJID, ADTM, EVID, SRCSEQ) %>%
  group_by(USUBJID) %>%
  mutate(ASEQ = row_number()) %>%
  ungroup() %>%
  mutate(
    MDV = if_else(is.na(DV), 1L, 0L),
    MDVM6 = if_else(is.na(DVM6), 1L, 0L)
  ) %>%
  select(STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, ALLOQ, CENS, DV, MDV, DVM6, MDVM6)

write_csv(adppk, "/app/output/adppk.csv", na = "")
