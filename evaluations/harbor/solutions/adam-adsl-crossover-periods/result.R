# Reference solution for the yamaa benchmark adam-adsl-crossover-periods (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(
    EXSEQ = col_integer(),
    EXSTDTC = col_date(),
    EXENDTC = col_date(),
    .default = col_character()
  )
)

ex1 <- ex |> filter(EPOCH %in% "TREATMENT 1")
ex2 <- ex |> filter(EPOCH %in% "TREATMENT 2")

agg1 <- ex1 |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    TR01SDT = if (all(is.na(EXSTDTC))) as.Date(NA) else min(EXSTDTC, na.rm = TRUE),
    TR01EDT = if (all(is.na(EXENDTC))) as.Date(NA) else max(EXENDTC, na.rm = TRUE),
    .groups = "drop"
  )
agg2 <- ex2 |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    TR02SDT = if (all(is.na(EXSTDTC))) as.Date(NA) else min(EXSTDTC, na.rm = TRUE),
    TR02EDT = if (all(is.na(EXENDTC))) as.Date(NA) else max(EXENDTC, na.rm = TRUE),
    .groups = "drop"
  )

# The treatment on the earliest record in each period: earliest start date
# with the lower sequence number breaking ties, records without a start
# date sorting last.
first1 <- ex1 |>
  arrange(STUDYID, USUBJID, is.na(EXSTDTC), EXSTDTC, EXSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, TRT01A = EXTRT)
first2 <- ex2 |>
  arrange(STUDYID, USUBJID, is.na(EXSTDTC), EXSTDTC, EXSEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, TRT02A = EXTRT)

adsl <- dm |>
  left_join(agg1, by = c("STUDYID", "USUBJID")) |>
  left_join(agg2, by = c("STUDYID", "USUBJID")) |>
  left_join(first1, by = c("STUDYID", "USUBJID")) |>
  left_join(first2, by = c("STUDYID", "USUBJID")) |>
  mutate(WASHDUR = as.integer(TR02SDT - TR01EDT) - 1L) |>
  select(
    STUDYID, USUBJID, TR01SDT, TR01EDT, TR02SDT, TR02EDT,
    TRT01A, TRT02A, WASHDUR
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
