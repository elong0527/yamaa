# Reference solution for the yamaa benchmark adam-adcm-on-treatment (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSEQ = col_integer(), .default = col_character())
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)

adcm <- cm |>
  mutate(ASTDT = CMSTDTC, AENDT = CMENDTC) |>
  left_join(adsl |> select(USUBJID, TRTSDT, TRTEDT), by = "USUBJID") |>
  mutate(
    # The medication overlaps the treatment period. A missing medication
    # date is assumed to overlap unless the known dates rule it out; a
    # missing treatment end leaves the period open, while no treatment
    # start is never flagged.
    ONTRTFL = case_when(
      is.na(TRTSDT) ~ NA_character_,
      !is.na(AENDT) & AENDT < TRTSDT ~ NA_character_,
      !is.na(ASTDT) & !is.na(TRTEDT) & ASTDT > TRTEDT ~ NA_character_,
      .default = "Y"
    )
  ) |>
  select(
    STUDYID, USUBJID, CMSEQ, CMTRT, ASTDT, AENDT, TRTSDT, TRTEDT, ONTRTFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adcm, "/app/output/adcm.csv", na = "")
