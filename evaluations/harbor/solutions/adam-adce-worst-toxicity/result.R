# Reference solution for the yamaa benchmark adam-adce-worst-toxicity (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ce <- read_csv(
  "/app/input/ce.csv",
  col_types = cols(CESEQ = col_integer(), ASTDT = col_date(), .default = col_character())
)

adce <- ce |>
  mutate(
    ASEV = CESEV,
    ASEVN = case_when(
      ASEV == "MILD" ~ 1L,
      ASEV == "MODERATE" ~ 2L,
      ASEV == "SEVERE" ~ 3L
    ),
    ATOXGRN = ASEVN
  )

# The graded event with the greatest grade per subject; ties break by
# earliest date, then lowest sequence, so at most one per subject.
best <- adce |>
  filter(!is.na(ATOXGRN)) |>
  arrange(USUBJID, desc(ATOXGRN), ASTDT, CESEQ) |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, CESEQ) |>
  mutate(BEST = "Y")

adce <- adce |>
  left_join(best, by = c("USUBJID", "CESEQ")) |>
  mutate(AOCCFL = BEST) |>
  select(STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adce, "/app/output/adce.csv", na = "")
