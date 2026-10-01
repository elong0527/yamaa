# Reference solution for the yamaa benchmark adam-adqs-subscale-score (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

qs <- read_csv(
  "/app/input/qs.csv",
  col_types = cols(
    QSSEQ = col_integer(),
    QSSTRESN = col_double(),
    .default = col_character()
  )
)

# Each physical functioning item response, on its zero to four scale.
items <- qs |>
  filter(QSTESTCD %in% c("PF01", "PF02", "PF03", "PF04")) |>
  mutate(PARAMCD = QSTESTCD, PARAM = QSTEST, AVAL = QSSTRESN) |>
  select(STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL)

# One score record per visit with a PF01 item record: the mean of the
# answered items on a zero to one hundred scale, with no value when fewer
# than three of the four items were answered.
visits <- items |>
  filter(PARAMCD %in% "PF01") |>
  distinct(STUDYID, USUBJID, VISIT)

stats <- items |>
  group_by(STUDYID, USUBJID, VISIT) |>
  summarise(
    n_answered = sum(!is.na(AVAL)),
    mean_answered = mean(AVAL, na.rm = TRUE),
    .groups = "drop"
  ) |>
  mutate(
    mean_answered = if_else(
      n_answered < 3 | is.nan(mean_answered),
      NA_real_,
      mean_answered * 25
    )
  )

scores <- visits |>
  left_join(stats, by = c("STUDYID", "USUBJID", "VISIT")) |>
  mutate(
    PARAMCD = "PFSCORE",
    PARAM = "Physical Functioning Subscale Score",
    AVAL = mean_answered
  ) |>
  select(STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL)

adqs <- bind_rows(items, scores) |>
  arrange(STUDYID, USUBJID, VISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adqs, "/app/output/adqs.csv", na = "")
