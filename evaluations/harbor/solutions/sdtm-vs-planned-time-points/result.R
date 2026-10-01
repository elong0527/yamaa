# Reference solution for the yamaa benchmark sdtm-vs-planned-time-points (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate)
library(readr)
library(tidyr)

study_day <- function(ref, coll) {
  if (is.na(ref) || is.na(coll)) {
    return(NA_integer_)
  }
  delta <- as.integer(coll - ref)
  if (delta >= 0L) delta + 1L else delta
}

time_point <- function(form) {
  name <- toupper(form)
  if (grepl("PREDOSE", name, fixed = TRUE)) {
    list(vstpt = "PRE-DOSE", vstptnum = 1L, vseltm = "-PT15M")
  } else if (grepl("30MIN", name, fixed = TRUE)) {
    list(vstpt = "30 MIN POST-DOSE", vstptnum = 2L, vseltm = "PT30M")
  } else if (grepl("1H", name, fixed = TRUE)) {
    list(vstpt = "1 H POST-DOSE", vstptnum = 3L, vseltm = "PT1H")
  } else if (grepl("4H", name, fixed = TRUE)) {
    list(vstpt = "4 H POST-DOSE", vstptnum = 4L, vseltm = "PT4H")
  } else {
    NULL
  }
}

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)
mapping <- read_csv(
  "/app/input/vs_mapping.csv",
  col_types = cols(.default = col_character())
)

ref_map <- setNames(suppressWarnings(ymd(dm$RFSTDTC)), dm$USUBJID)
test_map <- setNames(mapping$VSTESTCD, mapping$ItemOID)
unit_map <- setNames(mapping$UNIT, mapping$ItemOID)

forms <- odm |>
  filter(StudyEventOID == "SE.D1", grepl("^FO\\.VS_", FormOID)) |>
  group_by(SubjectKey, FormOID, StudyOID) |>
  summarise(
    vsdtc = Value[ItemOID == "IT.VS.VSDTC"][1],
    .groups = "drop"
  )

results <- odm |>
  filter(
    StudyEventOID == "SE.D1",
    grepl("^FO\\.VS_", FormOID),
    ItemOID %in% names(test_map),
    !is.na(Value) & Value != ""
  ) |>
  left_join(forms, by = c("SubjectKey", "FormOID", "StudyOID")) |>
  rowwise() |>
  mutate(
    point = list(time_point(FormOID)),
    VSTPT = point$vstpt,
    VSTPTNUM = point$vstptnum,
    VSELTM = point$vseltm,
    VSTESTCD = test_map[[ItemOID]],
    VSORRES = Value,
    VSORRESU = unit_map[[ItemOID]],
    VSSTRESN = suppressWarnings(as.numeric(Value)),
    VSSTRESU = VSORRESU,
    VSDTC = vsdtc,
    ref = ref_map[[SubjectKey]],
    VSDY = study_day(ref, suppressWarnings(as_date(ymd_hms(VSDTC)))),
    test_ord = case_when(
      VSTESTCD == "PULSE" ~ 1L,
      VSTESTCD == "SYSBP" ~ 2L,
      .default = 3L
    )
  ) |>
  ungroup() |>
  filter(!is.na(VSTPT), !is.na(VSDTC) & VSDTC != "") |>
  arrange(SubjectKey, VSTPTNUM, test_ord) |>
  group_by(SubjectKey) |>
  mutate(VSSEQ = row_number()) |>
  ungroup() |>
  arrange(SubjectKey, VSSEQ) |>
  transmute(
    DOMAIN = "VS",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    VSSEQ,
    VISIT = "DAY 1",
    VSTPT, VSTPTNUM, VSELTM, VSTESTCD, VSORRES, VSORRESU, VSSTRESN, VSSTRESU,
    VSDTC, VSDY
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(results, "/app/output/vs.csv", na = "")
