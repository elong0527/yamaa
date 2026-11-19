# Reference solution for the yamaa benchmark adam-adppk-event-records (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
  library(stringr)
})

ex <- read_csv("/app/input/ex.csv", col_types = cols(.default = col_character()), na = c("", "NA"))
pc <- read_csv("/app/input/pc.csv", col_types = cols(.default = col_character()), na = c("", "NA"))

full_dtm <- function(x) {
  x <- str_trim(x)
  d <- "[0-9]{4}-[0-9]{2}-[0-9]{2}"
  case_when(
    is.na(x) ~ NA_character_,
    str_detect(x, paste0("^", d, "$")) ~ paste0(x, "T00:00:00"),
    str_detect(x, paste0("^", d, "T[0-9]{2}$")) ~ paste0(x, ":00:00"),
    str_detect(x, paste0("^", d, "T[0-9]{2}:[0-9]{2}$")) ~ paste0(x, ":00"),
    TRUE ~ str_sub(x, 1, 19)
  )
}

doses <- ex %>%
  mutate(AMT = as.numeric(EXDOSE)) %>%
  filter(!is.na(AMT), AMT > 0) %>%
  transmute(
    STUDYID, USUBJID,
    ADTM = full_dtm(EXSTDTC),
    EVID = 1L, CMT = 1L, PARAMCD = "DOSE",
    AMT, DV = NA_real_,
    SRCSEQ = as.numeric(EXSEQ)
  )

concs <- pc %>%
  filter(is.na(PCSTAT) | str_to_upper(str_trim(PCSTAT)) != "NOT DONE") %>%
  semi_join(doses, by = "USUBJID") %>%
  transmute(
    STUDYID, USUBJID,
    ADTM = full_dtm(PCDTC),
    EVID = 0L,
    CMT = case_when(PCTESTCD == "DRGX" ~ 2L, PCTESTCD == "MET1" ~ 3L, TRUE ~ NA_integer_),
    PARAMCD = PCTESTCD,
    AMT = NA_real_,
    DV = if_else(!is.na(PCSTRESC) & str_detect(PCSTRESC, "^ *<"), NA_real_, as.numeric(PCSTRESN)),
    SRCSEQ = as.numeric(PCSEQ)
  )

adppk <- bind_rows(doses, concs) %>%
  mutate(MDV = if_else(is.na(DV), 1L, 0L)) %>%
  arrange(USUBJID, ADTM, EVID, CMT, SRCSEQ) %>%
  mutate(ID = dense_rank(USUBJID)) %>%
  group_by(USUBJID) %>%
  mutate(ASEQ = row_number()) %>%
  ungroup() %>%
  select(STUDYID, USUBJID, ID, ASEQ, ADTM, EVID, CMT, PARAMCD, AMT, DV, MDV)

write_csv(adppk, "/app/output/adppk.csv", na = "")
