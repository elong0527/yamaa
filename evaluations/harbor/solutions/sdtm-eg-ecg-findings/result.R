# Reference solution for the yamaa benchmark sdtm-eg-ecg-findings (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character()),
  na = ""
)

# One replicate ECG is one repeat of the ECG item group on a time point's
# form; its items become columns.
ecgs <- odm |>
  filter(FormOID %in% c("FO.EG_PREDOSE", "FO.EG_2H")) |>
  select(StudyOID, SubjectKey, StudyEventOID, FormOID, ItemGroupRepeatKey,
         ItemOID, Value) |>
  pivot_wider(names_from = ItemOID, values_from = Value)

# One record per test; a blank result gives no record.
qt <- ecgs |>
  transmute(StudyOID, SubjectKey, StudyEventOID, FormOID, ItemGroupRepeatKey,
            EGDTC = IT.EG.EGDTC,
            EGTESTCD = "QTAG", EGTEST = "QT Interval, Aggregate",
            EGORRES = IT.EG.QT, EGORRESU = "msec")
interpretation <- ecgs |>
  transmute(StudyOID, SubjectKey, StudyEventOID, FormOID, ItemGroupRepeatKey,
            EGDTC = IT.EG.EGDTC,
            EGTESTCD = "INTP", EGTEST = "Interpretation",
            EGORRES = IT.EG.INTP, EGORRESU = NA_character_)

eg <- bind_rows(qt, interpretation) |>
  filter(!is.na(EGORRES), trimws(EGORRES) != "") |>
  mutate(
    DOMAIN = "EG",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    EGSTRESC = EGORRES,
    EGSTRESN = if_else(EGTESTCD == "QTAG", suppressWarnings(as.numeric(EGORRES)),
                       NA_real_),
    EGSTRESU = EGORRESU,
    EGREPNUM = as.integer(ItemGroupRepeatKey),
    VISIT = case_when(StudyEventOID == "SE.DAY1" ~ "DAY 1"),
    EGTPT = case_when(
      FormOID == "FO.EG_PREDOSE" ~ "PRE-DOSE",
      FormOID == "FO.EG_2H" ~ "2 H POST-DOSE"
    ),
    EGTPTNUM = case_when(
      FormOID == "FO.EG_PREDOSE" ~ 1L,
      FormOID == "FO.EG_2H" ~ 2L
    )
  ) |>
  arrange(STUDYID, USUBJID, EGTPTNUM, EGREPNUM, EGTESTCD) |>
  group_by(STUDYID, USUBJID) |>
  mutate(EGSEQ = row_number()) |>
  ungroup() |>
  select(DOMAIN, STUDYID, USUBJID, EGSEQ, EGTESTCD, EGTEST, EGORRES, EGORRESU,
         EGSTRESC, EGSTRESN, EGSTRESU, EGREPNUM, VISIT, EGDTC, EGTPT,
         EGTPTNUM)

dir.create("/app/output", showWarnings = FALSE)
write_csv(eg, "/app/output/eg.csv", na = "")
