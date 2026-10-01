# Reference solution for the yamaa benchmark adam-advs-bmi (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(VSSTRESN = col_double(), .default = col_character())
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(HEIGHTBL = col_double(), .default = col_character())
)

# Every collected result stays in place.
collected <- vs |>
  select(STUDYID, USUBJID, AVISIT = VISIT, PARAMCD = VSTESTCD, PARAM = VSTEST, AVAL = VSSTRESN)

# One BMI record for each weight with a result, using the subject's
# baseline height in metres. Without a usable height the record stays
# with no value.
weights <- vs |>
  filter(VSTESTCD %in% "WEIGHT", !is.na(VSSTRESN)) |>
  left_join(adsl |> select(USUBJID, HEIGHTBL), by = "USUBJID") |>
  mutate(
    BMI = if_else(
      !is.na(HEIGHTBL) & HEIGHTBL != 0,
      VSSTRESN / ((HEIGHTBL / 100) ^ 2),
      NA_real_
    )
  ) |>
  transmute(
    STUDYID, USUBJID, AVISIT = VISIT,
    PARAMCD = "BMI", PARAM = "Body Mass Index (kg/m^2)", AVAL = BMI
  )

advs <- bind_rows(collected, weights) |>
  arrange(USUBJID, AVISIT, PARAMCD)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
