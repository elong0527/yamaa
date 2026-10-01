# Reference solution for the yamaa benchmark sdtm-vs-replicates (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

fmt_text <- function(x) {
  if (is.na(x)) {
    return(NA_character_)
  }
  if (x == floor(x)) {
    formatC(x, format = "f", digits = 0)
  } else {
    format(x, scientific = FALSE, trim = TRUE, digits = 15)
  }
}

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)

readings <- odm |>
  filter(
    ItemOID %in% c("IT.VS.SYSBP", "IT.VS.DIABP"),
    StudyEventOID %in% c("SE.VISIT1", "SE.VISIT2"),
    !is.na(Value) & Value != ""
  ) |>
  mutate(
    VISIT = case_when(
      StudyEventOID == "SE.VISIT1" ~ "VISIT 1",
      .default = "VISIT 2"
    ),
    VSTESTCD = case_when(
      ItemOID == "IT.VS.DIABP" ~ "DIABP",
      .default = "SYSBP"
    ),
    VSTEST = case_when(
      ItemOID == "IT.VS.DIABP" ~ "Diastolic Blood Pressure",
      .default = "Systolic Blood Pressure"
    ),
    VSREPNUM = suppressWarnings(as.integer(ItemGroupRepeatKey)),
    val = suppressWarnings(as.numeric(Value)),
    STUDYID = StudyOID,
    USUBJID = SubjectKey
  ) |>
  filter(!is.na(val), !is.na(VSREPNUM)) |>
  select(STUDYID, USUBJID, VISIT, VSTESTCD, VSTEST, VSREPNUM, val)

mean_records <- readings |>
  group_by(STUDYID, USUBJID, VISIT, VSTESTCD, VSTEST) |>
  summarise(mean_val = mean(val), .groups = "drop")

vs <- bind_rows(
  readings |>
    transmute(
      DOMAIN = "VS",
      STUDYID, USUBJID, VISIT, VSTESTCD, VSTEST,
      VSREPNUM,
      VSORRES = vapply(val, fmt_text, character(1)),
      VSORRESU = "mmHg",
      VSSTRESN = val,
      VSSTRESC = VSORRES,
      VSDRVFL = NA_character_
    ),
  mean_records |>
    transmute(
      DOMAIN = "VS",
      STUDYID, USUBJID, VISIT, VSTESTCD, VSTEST,
      VSREPNUM = NA_integer_,
      VSORRES = vapply(mean_val, fmt_text, character(1)),
      VSORRESU = "mmHg",
      VSSTRESN = mean_val,
      VSSTRESC = VSORRES,
      VSDRVFL = "Y"
    )
) |>
  arrange(USUBJID, VISIT, VSTESTCD, is.na(VSREPNUM), VSREPNUM) |>
  group_by(USUBJID) |>
  mutate(VSSEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, VSSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM,
    VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
