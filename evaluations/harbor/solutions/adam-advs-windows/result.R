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

# A planned visit with no collected record whose study day falls in its
# window still appears as an expected record: the planned visit name and
# number and the window, a continued sequence number, and no date, study
# day, or value. The open-ended post-treatment window never gets one.
planned <- data.frame(
  AVISIT = c("SCREENING", "BASELINE", "WEEK 2", "WEEK 4"),
  VISIT = c("SCREENING", "BASELINE", "WEEK 2", "WEEK 4"),
  VISITNUM = c(1, 2, 3, 4),
  AVISITN = c(-1L, 0L, 2L, 4L),
  lo = c(-Inf, 1, 2, 22),
  hi = c(-1, 1, 21, 42),
  stringsAsFactors = FALSE
)

expected <- merge(distinct(framed, STUDYID, USUBJID, PARAMCD), planned) |>
  rowwise() |>
  mutate(
    covered = {
      days <- framed$ADY[
        framed$STUDYID == STUDYID &
          framed$USUBJID == USUBJID &
          framed$PARAMCD == PARAMCD &
          !is.na(framed$ADY)
      ]
      any(days >= lo & days <= hi)
    },
    top_seq = max(
      framed$VSSEQ[
        framed$STUDYID == STUDYID &
          framed$USUBJID == USUBJID &
          framed$PARAMCD == PARAMCD
      ]
    )
  ) |>
  ungroup() |>
  filter(!covered) |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  arrange(match(AVISIT, planned$AVISIT), .by_group = TRUE) |>
  mutate(
    VSSEQ = top_seq + row_number(),
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
