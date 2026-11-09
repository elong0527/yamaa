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

wide <- odm |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  )
for (column in c(
  "IT.DM.SITEID", "IT.DM.SEX", "IT.DM.AGE", "IT.DM.ARM", "IT.DM.COUNTRY"
)) {
  if (!column %in% names(wide)) wide[[column]] <- NA_character_
}
dm <- wide |>
  mutate(
    DOMAIN = "DM",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    SUBJID = SubjectKey,
    SITEID = `IT.DM.SITEID`,
    # M for Male, F for Female; anything missing, blank, uncollected, or
    # otherwise recorded becomes U.
    SEX = case_when(
      `IT.DM.SEX` == "Male" ~ "M",
      `IT.DM.SEX` == "Female" ~ "F",
      .default = "U"
    ),
    AGE = suppressWarnings(as.integer(`IT.DM.AGE`)),
    AGEU = if_else(is.na(AGE), NA_character_, "YEARS"),
    ARM = na_if(str_trim(`IT.DM.ARM`), ""),
    # The actual arm always equals the planned arm.
    ACTARM = ARM,
    ARMNRS = if_else(is.na(ARM), "NOT ASSIGNED", NA_character_),
    COUNTRY = `IT.DM.COUNTRY`
  ) |>
  arrange(USUBJID) |>
  select(
    DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, ARM, ACTARM,
    ARMNRS, COUNTRY
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
