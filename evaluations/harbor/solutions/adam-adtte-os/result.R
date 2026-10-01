# Reference solution for the yamaa benchmark adam-adtte-os (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(RANDDT = col_date(), LSTALVDT = col_date(), .default = col_character())
)
adrs <- read_csv(
  "/app/input/adrs.csv",
  col_types = cols(ASEQ = col_integer(), ADT = col_date(), .default = col_character())
)

# The earliest dated qualifying death: code DEATH, result Y, flag Y.
# On a tied date the lower sequence wins.
death <- adrs |>
  filter(PARAMCD == "DEATH", AVALC == "Y", ANL01FL == "Y", !is.na(ADT)) |>
  arrange(STUDYID, USUBJID, ADT, ASEQ) |>
  distinct(STUDYID, USUBJID, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, DEATHDT = ADT, DEATHSEQ = ASEQ)

adtte <- adsl |>
  left_join(death, by = c("STUDYID", "USUBJID")) |>
  mutate(
    STARTDT = RANDDT,
    CENSORDT1 = LSTALVDT,
    # A death before randomization still counts, moved up to it; with
    # no death, the last-alive date when after randomization, else
    # randomization itself.
    ADT = case_when(
      !is.na(DEATHDT) & !is.na(STARTDT) & DEATHDT >= STARTDT ~ DEATHDT,
      !is.na(DEATHDT) ~ STARTDT,
      !is.na(CENSORDT1) & !is.na(STARTDT) & CENSORDT1 > STARTDT ~ CENSORDT1,
      .default = STARTDT
    ),
    AVAL = as.integer(ADT - STARTDT) + 1L,
    CNSR = if_else(!is.na(DEATHDT), 0L, 1L),
    EVNTDESC = case_when(
      !is.na(DEATHDT) ~ "Death",
      !is.na(CENSORDT1) & !is.na(STARTDT) & CENSORDT1 > STARTDT ~ "Alive",
      .default = "Randomization"
    ),
    CNSDTDSC = case_when(
      !is.na(DEATHDT) ~ NA_character_,
      !is.na(CENSORDT1) & !is.na(STARTDT) & CENSORDT1 > STARTDT ~ "Alive During Study",
      .default = "Randomization"
    ),
    SRCDOM = if_else(!is.na(DEATHDT), "ADRS", "ADSL"),
    SRCVAR = case_when(
      !is.na(DEATHDT) ~ "ADT",
      !is.na(CENSORDT1) & !is.na(STARTDT) & CENSORDT1 > STARTDT ~ "LSTALVDT",
      .default = "RANDDT"
    ),
    SRCSEQ = if_else(!is.na(DEATHDT), DEATHSEQ, NA_integer_),
    PARAMCD = "OS",
    PARAM = "Overall Survival"
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR,
    EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ
  ) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adtte, "/app/output/adtte.csv", na = "")
