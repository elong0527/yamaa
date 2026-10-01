# Reference solution for the yamaa benchmark adam-adtr-sum (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

trvisit <- read_csv(
  "/app/input/trvisit.csv",
  col_types = cols(AVISITN = col_integer(), ADT = col_date(), .default = col_character())
)
tr <- read_csv(
  "/app/input/tr.csv",
  col_types = cols(TRSTRESN = col_double(), .default = col_character())
)
tu <- read_csv(
  "/app/input/tu.csv",
  col_types = cols(.default = col_character())
)

# Lesions selected as target at study entry: the same count at every
# assessment of the subject.
ntarget <- tu |>
  filter(TUGRPID == "TARGET") |>
  count(STUDYID, USUBJID, name = "NTARGET")

# Target longest diameters at each assessment. A record with no result
# contributes nothing, while a zero counts as measured.
agg <- tr |>
  filter(TRGRPID == "TARGET", TRTESTCD == "LDIAM") |>
  group_by(STUDYID, USUBJID, AVISIT) |>
  summarise(
    AVAL_sum = sum(TRSTRESN, na.rm = TRUE),
    NMEAS = sum(!is.na(TRSTRESN)),
    .groups = "drop"
  ) |>
  mutate(AVAL_sum = if_else(NMEAS == 0L, NA_real_, AVAL_sum))

adtr <- trvisit |>
  left_join(agg, by = c("STUDYID", "USUBJID", "AVISIT")) |>
  left_join(ntarget, by = c("STUDYID", "USUBJID")) |>
  mutate(
    AVAL = AVAL_sum,
    PARAMCD = "SDIAM",
    PARAM = "Sum of Target Lesion Diameters (mm)",
    ANL01FL = if_else(!is.na(NMEAS) & !is.na(NTARGET) & NMEAS == NTARGET, "Y", NA_character_)
  ) |>
  select(
    STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM,
    AVAL, NMEAS, NTARGET, ANL01FL
  ) |>
  arrange(USUBJID, AVISITN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtr, "/app/output/adtr.csv", na = "")
