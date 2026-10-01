# Reference solution for the yamaa benchmark sdtm-ex-from-ec (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)
kit <- read_csv(
  "/app/input/kit_list.csv",
  col_types = cols(.default = col_character())
)

kit_trt <- setNames(kit$TREATMENT, kit$KIT)
kit_dose <- setNames(suppressWarnings(as.numeric(kit$DOSE_MG)), kit$KIT)

ec_forms <- odm |> filter(FormOID == "FO.EC")
vs_weight <- odm |>
  filter(FormOID == "FO.VS", ItemOID == "IT.VS.WEIGHT") |>
  group_by(StudyOID, SubjectKey, StudyEventOID, StudyEventRepeatKey) |>
  summarise(WEIGHT = suppressWarnings(as.numeric(first(Value))), .groups = "drop")

ec_wide <- ec_forms |>
  group_by(StudyOID, SubjectKey, StudyEventOID, StudyEventRepeatKey, FormRepeatKey) |>
  summarise(
    ECOCCUR = Value[ItemOID == "IT.EC.ECOCCUR"][1],
    ECTRT = Value[ItemOID == "IT.EC.ECTRT"][1],
    FORM = Value[ItemOID == "IT.EC.FORM"][1],
    TABLETS = suppressWarnings(as.numeric(Value[ItemOID == "IT.EC.TABLETS"][1])),
    STRENGTHMG = suppressWarnings(as.numeric(Value[ItemOID == "IT.EC.STRENGTHMG"][1])),
    DOSEMKG = suppressWarnings(as.numeric(Value[ItemOID == "IT.EC.DOSEMKG"][1])),
    KIT = Value[ItemOID == "IT.EC.KIT"][1],
    AUCTARGET = suppressWarnings(as.numeric(Value[ItemOID == "IT.EC.AUCTARGET"][1])),
    ECSTDTC = Value[ItemOID == "IT.EC.ECSTDTC"][1],
    ECENDTC = Value[ItemOID == "IT.EC.ECENDTC"][1],
    .groups = "drop"
  ) |>
  left_join(vs_weight, by = c("StudyOID", "SubjectKey", "StudyEventOID", "StudyEventRepeatKey")) |>
  filter(ECOCCUR == "Y")

ex <- ec_wide |>
  rowwise() |>
  mutate(
    DOMAIN = "EX",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    EXTRT = if (FORM == "KITDOSE") unname(kit_trt[KIT]) else ECTRT,
    EXDOSE = case_when(
      FORM == "TABLET" ~ TABLETS * STRENGTHMG,
      FORM == "INFUSION" ~ DOSEMKG * WEIGHT,
      FORM == "KITDOSE" ~ unname(kit_dose[KIT]),
      FORM == "AUCDOSE" ~ AUCTARGET,
      .default = NA_real_
    ),
    EXDOSU = ifelse(FORM == "AUCDOSE", "AUC", "mg"),
    FORMREPEAT = suppressWarnings(as.integer(FormRepeatKey)),
    EXSTDTC_DATE = suppressWarnings(as.Date(na_if(ECSTDTC, "")))
  ) |>
  ungroup() |>
  arrange(STUDYID, USUBJID, EXSTDTC_DATE, FORMREPEAT) |>
  group_by(STUDYID, USUBJID) |>
  mutate(EXSEQ = row_number()) |>
  ungroup() |>
  select(DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC = ECSTDTC, EXENDTC = ECENDTC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ex, "/app/output/ex.csv", na = "")
