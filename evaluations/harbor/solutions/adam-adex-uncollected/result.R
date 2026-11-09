# Reference solution for the yamaa benchmark adam-adex-uncollected (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
ex <- read_csv(
  "/app/input/ex.csv",
  col_types = cols(EXDOSE = col_double(), .default = col_character())
)

if (any(!is.na(ex$EXDOSE) & (is.na(ex$EXDOSU) | ex$EXDOSU != "mg"))) {
  stop("Recorded doses must be standardized to mg")
}

# Per subject: total dose, record count, and count of recorded doses.
agg <- ex |>
  group_by(STUDYID, USUBJID) |>
  summarise(
    DOSE_SUM = sum(EXDOSE, na.rm = TRUE),
    NDOSREC = n(),
    NDOSVAL = sum(!is.na(EXDOSE)),
    .groups = "drop"
  ) |>
  mutate(DOSECUM = if_else(NDOSVAL > 0, DOSE_SUM, NA_real_)) |>
  select(STUDYID, USUBJID, DOSECUM, NDOSREC, NDOSVAL)

adex <- adsl |>
  left_join(agg, by = c("STUDYID", "USUBJID")) |>
  select(STUDYID, USUBJID, TRT01P, DOSECUM, NDOSREC, NDOSVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adex, "/app/output/adex.csv", na = "")
