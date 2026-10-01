# Reference solution for the yamaa benchmark sdtm-pr-procedures (R track).
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

wide <- odm |>
  mutate(Value = na_if(Value, "")) |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, StudyEventOID, ItemGroupOID, ItemGroupRepeatKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = function(x) x[[1]]
  )

pr <- wide |>
  mutate(
    STUDYID = StudyOID,
    # USUBJID carries the study prefix when the ODM subject key does not.
    USUBJID = if_else(
      startsWith(SubjectKey, paste0(StudyOID, "-")),
      SubjectKey, paste0(StudyOID, "-", SubjectKey)
    ),
    DOMAIN = "PR",
    PRTRT = IT.PR.PRTRT,
    PRCAT = case_when(
      ItemGroupOID == "IG.PR.SURGERY" ~ "PRIOR CANCER SURGERY",
      ItemGroupOID == "IG.PR.RADIOTHERAPY" ~ "PRIOR RADIOTHERAPY"
    ),
    PRPRESP = if_else(ItemGroupOID == "IG.PR.RADIOTHERAPY", "Y", NA_character_),
    PROCCUR = if_else(
      ItemGroupOID == "IG.PR.RADIOTHERAPY", IT.PR.RTPRSP, NA_character_
    ),
    PRLOC = if_else(ItemGroupOID == "IG.PR.SURGERY", IT.PR.PRLOC, NA_character_),
    PRLAT = if_else(ItemGroupOID == "IG.PR.SURGERY", IT.PR.PRLAT, NA_character_),
    PRDOSE = if_else(
      ItemGroupOID == "IG.PR.RADIOTHERAPY" & PROCCUR == "Y",
      IT.PR.PRDOSE, NA_character_
    ),
    PRDOSU = if_else(
      ItemGroupOID == "IG.PR.RADIOTHERAPY" & PROCCUR == "Y",
      IT.PR.PRDOSU, NA_character_
    ),
    PRSTDTC = case_when(
      ItemGroupOID == "IG.PR.RADIOTHERAPY" & PROCCUR == "Y" ~ IT.PR.PRSTDTC,
      ItemGroupOID == "IG.PR.SURGERY" ~ IT.PR.PRSTDTC,
      TRUE ~ NA_character_
    ),
    PRENDTC = if_else(
      ItemGroupOID == "IG.PR.RADIOTHERAPY" & PROCCUR == "Y",
      IT.PR.PRENDTC, NA_character_
    )
  ) |>
  arrange(USUBJID, is.na(PRSTDTC), PRSTDTC, PRTRT) |>
  group_by(USUBJID) |>
  mutate(PRSEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, PRSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR,
    PRLOC, PRLAT, PRDOSE, PRDOSU, PRSTDTC, PRENDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(pr, "/app/output/pr.csv", na = "")
