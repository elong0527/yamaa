# Reference solution for the yamaa benchmark adam-advs-windows (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/advs_raw.csv",
  col_types = cols(
    VSSEQ = col_integer(), VISITNUM = col_double(), ADT = col_date(),
    ADY = col_integer(), AVAL = col_double(), .default = col_character()
  )
)

# Windows follow the study day: screening before day 0, baseline on day
# 1, weeks 2 and 4 on days 2-21 and 22-42, and post-treatment from day
# 43 on with no upper bound. No study day means no window.
framed <- raw |>
  mutate(
    AVISIT = case_when(
      is.na(ADY) ~ NA_character_,
      ADY < 0 ~ "SCREENING",
      ADY < 2 ~ "BASELINE",
      ADY < 22 ~ "WEEK 2",
      ADY < 43 ~ "WEEK 4",
      .default = "POST-TREATMENT"
    ),
    AVISITN = case_when(
      AVISIT %in% "SCREENING" ~ -1L,
      AVISIT %in% "BASELINE" ~ 0L,
      AVISIT %in% "WEEK 2" ~ 2L,
      AVISIT %in% "WEEK 4" ~ 4L,
      AVISIT %in% "POST-TREATMENT" ~ 99L
    )
  )

# The earliest record by study day in each study, subject, parameter,
# and visit; the lower sequence number breaks a same-day tie.
flagged <- framed |>
  filter(!is.na(AVISIT)) |>
  arrange(ADY, VSSEQ) |>
  distinct(STUDYID, USUBJID, PARAMCD, AVISIT, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, PARAMCD, AVISIT, VSSEQ) |>
  mutate(ANL01FL = "Y")

advs <- framed |>
  left_join(flagged, by = c("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "VSSEQ")) |>
  arrange(STUDYID, USUBJID, VSSEQ) |>
  select(
    STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
    AVISIT, AVISITN, ANL01FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
