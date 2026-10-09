# Reference solution for the yamaa benchmark adam-adqs-multi-scale-scoring (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr, warn.conflicts = FALSE)
library(readr)

qs <- read_csv(
  "/app/input/qs.csv",
  col_types = cols(
    QSSEQ = col_integer(),
    QSSTRESN = col_double(),
    .default = col_character()
  )
)

# Each item response, on its own answer scale.
items <- qs |>
  mutate(PARAMCD = QSTESTCD, PARAM = QSTEST, AVAL = QSSTRESN) |>
  select(STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL)

# The four scales: member items, anchor item, minimum answered items,
# direction (down = higher score means better, up = higher means worse),
# and the answer range.
scales <- tribble(
  ~pcode, ~pname, ~members, ~anchor, ~min_ans, ~direction, ~rng,
  "F1SCORE", "Physical Functioning Scale Score",
  list(c("F101", "F102", "F103", "F104")), "F101", 2L, "down", 3,
  "F2SCORE", "Role Functioning Scale Score",
  list(c("F201", "F202")), "F201", 1L, "down", 3,
  "SSCORE", "Fatigue and Sleep Symptom Scale Score",
  list(c("S01", "S02", "F104")), "S01", 2L, "up", 3,
  "GSCORE", "Global Health Scale Score",
  list(c("G01", "G02")), "G01", 1L, "up", 6
)

score_one <- function(pcode, pname, members, anchor, min_ans, direction, rng) {
  # `members` arrives as a one-element list from the tribble's list-column;
  # `%in%` against a list never matches, so unwrap it first.
  members <- unlist(members)
  member_rows <- items |> filter(PARAMCD %in% members)
  visits <- items |>
    filter(PARAMCD %in% anchor) |>
    distinct(STUDYID, USUBJID, VISIT)
  stats <- member_rows |>
    group_by(STUDYID, USUBJID, VISIT) |>
    summarise(
      n_answered = sum(!is.na(AVAL)),
      raw_mean = mean(AVAL, na.rm = TRUE),
      .groups = "drop"
    )
  visits |>
    left_join(stats, by = c("STUDYID", "USUBJID", "VISIT")) |>
    mutate(
      AVAL = dplyr::case_when(
        is.na(n_answered) | n_answered < min_ans ~ NA_real_,
        direction == "down" ~ 100 * (1 - (raw_mean - 1) / rng),
        TRUE ~ 100 * (raw_mean - 1) / rng
      ),
      PARAMCD = pcode,
      PARAM = pname
    ) |>
    select(STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL)
}

scores <- pmap_dfr(scales, score_one)

adqs <- bind_rows(items, scores) |>
  arrange(STUDYID, USUBJID, VISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adqs, "/app/output/adqs.csv", na = "")
