# Reference solution for the yamaa benchmark adam-advs-windows (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

# SDTM VS staged as the ADaM input; rename to the analysis names the
# rest of the solution works with.
raw <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(), VISITNUM = col_double(), VSDTC = col_date(),
    VSDY = col_integer(), VSSTRESN = col_double(), .default = col_character()
  )
) |>
  rename(PARAMCD = VSTESTCD, ADT = VSDTC, ADY = VSDY, AVAL = VSSTRESN)

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

# SV lists every planned visit of every subject, including those that did
# not take place, and the unscheduled visits.
sv <- read_csv(
  "/app/input/sv.csv",
  col_types = cols(VISITNUM = col_double(), .default = col_character())
)

# Each planned SCREENING, BASELINE, WEEK 2, or WEEK 4 visit gets one SYSBP
# expected record when no collected record's study day falls in the
# analysis window of that name: the planned visit name and number and the
# window, and no date, study day, or value. No other visit gets one.
# Expected records continue the subject's sequence numbering after the
# highest collected VSSEQ, in VISITNUM order.
windows <- c("SCREENING" = -1L, "BASELINE" = 0L, "WEEK 2" = 2L, "WEEK 4" = 4L)

top <- framed |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  summarise(top_seq = max(VSSEQ), .groups = "drop")

expected <- sv |>
  filter(VISIT %in% names(windows)) |>
  mutate(
    PARAMCD = "SYSBP", AVISIT = VISIT, AVISITN = unname(windows[VISIT])
  ) |>
  anti_join(
    filter(framed, !is.na(AVISIT)),
    by = c("STUDYID", "USUBJID", "PARAMCD", "AVISIT")
  ) |>
  left_join(top, by = c("STUDYID", "USUBJID", "PARAMCD")) |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  arrange(VISITNUM, .by_group = TRUE) |>
  mutate(
    VSSEQ = coalesce(top_seq, 0L) + row_number(),
    ADT = as.Date(NA),
    ADY = NA_integer_,
    AVAL = NA_real_
  ) |>
  ungroup() |>
  select(
    STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
    AVISIT, AVISITN
  )

framed <- bind_rows(framed, expected)

# The earliest record by study day in each study, subject, parameter,
# and visit; the lower sequence number breaks a same-day tie. Expected
# records have no study day and never take the flag.
flagged <- framed |>
  filter(!is.na(AVISIT), !is.na(ADY)) |>
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
