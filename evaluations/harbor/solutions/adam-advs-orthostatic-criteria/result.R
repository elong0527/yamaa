# Reference solution for the yamaa benchmark
# adam-advs-orthostatic-criteria (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(), VSSTRESN = col_double(),
    .default = col_character()
  )
)

# The one reading of a test in a position at a visit; no such reading,
# or a reading with no result, gives no value.
only_value <- function(mask, values) {
  picked <- values[which(mask)]
  if (length(picked) > 1) {
    stop("more than one reading for a test, position and visit")
  }
  if (length(picked) == 0) NA_real_ else picked
}

standing <- "AFTER 3 MIN STANDING"
visits <- vs |>
  group_by(STUDYID, USUBJID, AVISIT = VISIT) |>
  summarise(
    SYSBP_SUP = only_value(VSTESTCD == "SYSBP" & VSPOS == "SUPINE", VSSTRESN),
    SYSBP_STD = only_value(
      VSTESTCD == "SYSBP" & VSPOS == "STANDING" & VSTPT == standing, VSSTRESN
    ),
    DIABP_SUP = only_value(VSTESTCD == "DIABP" & VSPOS == "SUPINE", VSSTRESN),
    DIABP_STD = only_value(
      VSTESTCD == "DIABP" & VSPOS == "STANDING" & VSTPT == standing, VSSTRESN
    ),
    HR_SUP = only_value(VSTESTCD == "HR" & VSPOS == "SUPINE", VSSTRESN),
    HR_STD = only_value(
      VSTESTCD == "HR" & VSPOS == "STANDING" & VSTPT == standing, VSSTRESN
    ),
    .groups = "drop"
  ) |>
  mutate(
    SYSBP_CHG = SYSBP_STD - SYSBP_SUP,
    DIABP_CHG = DIABP_STD - DIABP_SUP,
    HR_CHG = HR_STD - HR_SUP
  )

change_record <- function(visits, paramcd, param, column) {
  visits |>
    filter(!is.na(.data[[column]])) |>
    transmute(
      STUDYID, USUBJID, AVISIT, PARAMCD = paramcd, PARAM = param,
      AVAL = .data[[column]], AVALC = NA_character_
    )
}

# Either blood-pressure fall is enough for Y, even when the other
# change is missing; N needs both changes, so a visit with neither
# fall and one change missing gets no record.
hypotension <- visits |>
  mutate(
    AVALC = case_when(
      (!is.na(SYSBP_CHG) & SYSBP_CHG <= -20) |
        (!is.na(DIABP_CHG) & DIABP_CHG <= -10) ~ "Y",
      !is.na(SYSBP_CHG) & !is.na(DIABP_CHG) ~ "N",
      .default = NA_character_
    )
  ) |>
  filter(!is.na(AVALC)) |>
  transmute(
    STUDYID, USUBJID, AVISIT, PARAMCD = "OHYPO",
    PARAM = "Orthostatic Hypotension", AVAL = NA_real_, AVALC
  )

tachycardia <- visits |>
  mutate(
    AVALC = case_when(
      !is.na(HR_CHG) & HR_CHG >= 30 ~ "Y",
      !is.na(HR_CHG) ~ "N",
      .default = NA_character_
    )
  ) |>
  filter(!is.na(AVALC)) |>
  transmute(
    STUDYID, USUBJID, AVISIT, PARAMCD = "OTACHY",
    PARAM = "Orthostatic Tachycardia", AVAL = NA_real_, AVALC
  )

advs <- bind_rows(
  change_record(
    visits, "SYSBPOC",
    "Systolic Blood Pressure Change on Standing (mmHg)", "SYSBP_CHG"
  ),
  change_record(
    visits, "DIABPOC",
    "Diastolic Blood Pressure Change on Standing (mmHg)", "DIABP_CHG"
  ),
  change_record(
    visits, "HROC", "Heart Rate Change on Standing (beats/min)", "HR_CHG"
  ),
  hypotension,
  tachycardia
) |>
  select(STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL, AVALC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
