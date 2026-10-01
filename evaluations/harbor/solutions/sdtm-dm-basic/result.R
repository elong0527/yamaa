# Reference solution for the yamaa benchmark sdtm-dm-basic (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(stringr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

dm <- odm |>
  filter(ItemGroupOID == "IG.DM") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    DOMAIN = "DM",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    SUBJID = SubjectKey,
    # M for Male, F for Female; anything missing, blank, uncollected, or
    # otherwise recorded becomes U.
    SEX = case_when(
      `IT.DM.SEX` == "Male" ~ "M",
      `IT.DM.SEX` == "Female" ~ "F",
      .default = "U"
    ),
    AGE = suppressWarnings(as.integer(`IT.DM.AGE`)),
    ARM = na_if(str_trim(`IT.DM.ARM`), ""),
    # The actual arm always equals the planned arm.
    ACTARM = ARM,
    ARMNRS = if_else(
      is.na(ARM), "Not assigned to treatment arm", NA_character_
    )
  ) |>
  arrange(USUBJID) |>
  select(DOMAIN, STUDYID, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM, ARMNRS)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
