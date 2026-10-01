# Reference solution for the yamaa benchmark sdtm-fa-fever (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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

fa <- vs |>
  filter(VSTESTCD == "TEMP", VSCAT == "REACTOGENICITY") |>
  mutate(
    FASEQ = VSSEQ,
    FAORRES = case_when(
      is.na(VSSTRESN) | is.na(VSSTRESU) | VSSTRESU != "C" ~ NA_character_,
      VSSTRESN >= 38 ~ "Y",
      .default = "N"
    ),
    DOMAIN = "FA",
    FATESTCD = "OCCUR",
    FATEST = "Occurrence Indicator",
    FACAT = "REACTOGENICITY",
    FASCAT = "SYSTEMIC",
    FAOBJ = "FEVER",
    FASTRESC = FAORRES
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FACAT, FASCAT,
    FAOBJ, FAORRES, FASTRESC, VSSTRESN
  ) |>
  arrange(STUDYID, USUBJID, FASEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(fa, "/app/output/fa.csv", na = "")
