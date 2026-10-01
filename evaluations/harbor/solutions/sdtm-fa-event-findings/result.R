# Reference solution for the yamaa benchmark sdtm-fa-event-findings (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(purrr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

long <- odm |>
  mutate(Value = na_if(Value, ""))

ae <- long |>
  filter(ItemGroupOID == "IG.AE") |>
  select(SubjectKey, StudyOID, StudyEventOID, StudyEventRepeatKey, ItemOID, Value) |>
  pivot_wider(names_from = ItemOID, values_from = Value) |>
  rename(
    AETERM = `IT.AE.AETERM`,
    AELNKID_AE = `IT.AE.AELNKID`
  )

fa_forms <- long |>
  filter(ItemGroupOID == "IG.FA") |>
  select(
    SubjectKey, StudyOID, StudyEventOID, StudyEventRepeatKey,
    ItemGroupRepeatKey, ItemOID, Value
  ) |>
  pivot_wider(names_from = ItemOID, values_from = Value) |>
  rename(
    LINK_FA = `IT.FA.AELNKID`,
    LOCATION = `IT.FA.LOCATION`,
    SIZE = `IT.FA.SIZE`,
    SIZEU = `IT.FA.SIZEU`,
    BIOPSY = `IT.FA.BIOPSY`,
    FADTC = `IT.FA.FADTC`
  )

fa <- fa_forms |>
  left_join(
    ae,
    by = c("SubjectKey", "StudyEventOID", "StudyEventRepeatKey")
  ) |>
  filter(!is.na(AETERM) & AETERM != "") |>
  mutate(
    LINK = coalesce(LINK_FA, AELNKID_AE),
    STUDYID = StudyOID.x,
    USUBJID = SubjectKey
  ) |>
  select(STUDYID, USUBJID, LINK, FADTC, SIZEU, AETERM, LOCATION, SIZE, BIOPSY) |>
  pivot_longer(
    cols = c(LOCATION, SIZE, BIOPSY),
    names_to = "WHICH",
    values_to = "FAORRES"
  ) |>
  filter(!is.na(FAORRES)) |>
  mutate(
    ORDER = case_when(
      WHICH == "LOCATION" ~ 0L,
      WHICH == "SIZE" ~ 1L,
      .default = 2L
    ),
    FATESTCD = case_when(
      WHICH == "LOCATION" ~ "LOC",
      WHICH == "SIZE" ~ "SIZE",
      .default = "BIOPSY"
    ),
    FATEST = case_when(
      WHICH == "LOCATION" ~ "Location",
      WHICH == "SIZE" ~ "Size",
      .default = "Biopsied"
    ),
    FAOBJ = AETERM,
    FACAT = "AE",
    IS_SIZE = WHICH == "SIZE",
    FAORRESU = if_else(IS_SIZE, SIZEU, NA_character_),
    FASTRESC = FAORRES,
    FASTRESN = if_else(
      IS_SIZE, suppressWarnings(as.numeric(FAORRES)), NA_real_
    ),
    FASTRESU = if_else(IS_SIZE, SIZEU, NA_character_),
    FALNKID = LINK,
    DOMAIN = "FA",
    TIE = FAORRES
  ) |>
  arrange(STUDYID, USUBJID, LINK, ORDER, TIE) |>
  group_by(STUDYID, USUBJID) |>
  mutate(FASEQ = row_number()) |>
  ungroup() |>
  select(
    DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT,
    FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(fa, "/app/output/fa.csv", na = "")
