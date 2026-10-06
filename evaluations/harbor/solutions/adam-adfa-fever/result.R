# Reference solution for the yamaa benchmark adam-adfa-fever (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(),
    VSSTRESN = col_double(),
    .default = col_character()
  )
)

# Only temperature records in the reactogenicity category qualify.
adfa <- vs |>
  filter(VSTESTCD == "TEMP", VSCAT == "REACTOGENICITY") |>
  mutate(
    DOMAIN = "ADFA",
    PARAMCD = "FEVER",
    PARAM = "Fever Occurrence",
    AVALC = case_when(
      VSSTRESU == "C" & VSSTRESN >= 38 ~ "Y",
      VSSTRESU == "C" & VSSTRESN < 38 ~ "N"
    ),
    AVAL = case_when(
      AVALC == "Y" ~ 1,
      AVALC == "N" ~ 0
    ),
    ADT = as.Date(substr(VSDTC, 1, 10)),
    SRCDOM = "VS",
    SRCVAR = "VSSTRESN",
    SRCSEQ = VSSEQ
  ) |>
  # Number each subject's records 1, 2, 3 ... in assessment-date order,
  # the collected sequence breaking ties on the same date.
  arrange(USUBJID, VSDTC, VSSEQ) |>
  group_by(STUDYID, USUBJID) |>
  mutate(ASEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, ASEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM,
    AVAL, AVALC, ADT, SRCDOM, SRCVAR, SRCSEQ
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adfa, "/app/output/adfa.csv", na = "")
