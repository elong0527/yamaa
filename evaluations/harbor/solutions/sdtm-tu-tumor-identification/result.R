# Reference solution for the yamaa benchmark sdtm-tu-tumor-identification (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

raw <- read_csv(
  "/app/input/tu_raw.csv",
  col_types = cols(
    TUSEQ = col_integer(),
    VISITNUM = col_integer(),
    .default = col_character()
  )
)

tu <- raw |>
  mutate(
    DOMAIN = "TU",
    TULNKID = case_when(
      LESION_CATEGORY == "Target" ~ paste0("T", LESION_NUM),
      LESION_CATEGORY == "Non-target" ~ paste0("NT", LESION_NUM),
      .default = paste0("NEW", LESION_NUM)
    ),
    TUTESTCD = "TUMIDENT",
    TUTEST = "Tumor Identification",
    TUORRES = case_when(
      LESION_CATEGORY == "Target" ~ "TARGET",
      LESION_CATEGORY == "Non-target" ~ "NON-TARGET",
      .default = "NEW"
    ),
    TUSTRESC = TUORRES,
    TULOC = str_to_upper(LOCATION),
    TULAT = str_to_upper(LATERALITY),
    TUMETHOD = METHOD,
    TUEVAL = EVALUATOR
  ) |>
  arrange(USUBJID, TUSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES,
    TUSTRESC, TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(tu, "/app/output/tu.csv", na = "")
