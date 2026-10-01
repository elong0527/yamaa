# Reference solution for the yamaa benchmark adam-adsl-new-therapy (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl_raw <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
)
cm <- read_csv(
  "/app/input/cm.csv",
  col_types = cols(CMSTDTC_IMP = col_date(), .default = col_character())
)
pr <- read_csv(
  "/app/input/pr.csv",
  col_types = cols(PRSTDTC = col_date(), .default = col_character())
)

# A qualifying medication starts on or after treatment start; a record
# with no start date is skipped. The completed start fills a year-month
# to the 15th, so the IMP column is the comparison date.
cm_min <- cm |>
  inner_join(select(adsl_raw, STUDYID, USUBJID, TRTSDT), by = c("STUDYID", "USUBJID")) |>
  filter(CMCAT == "ON TREATMENT", !is.na(CMSTDTC_IMP), CMSTDTC_IMP >= TRTSDT) |>
  group_by(STUDYID, USUBJID) |>
  summarise(CMNACTDT = min(CMSTDTC_IMP), .groups = "drop")

# A qualifying procedure is cancer related and on treatment, also on or
# after treatment start.
pr_min <- pr |>
  inner_join(select(adsl_raw, STUDYID, USUBJID, TRTSDT), by = c("STUDYID", "USUBJID")) |>
  filter(
    PRCAT == "CANCER RELATED", PRSCAT == "ON TREATMENT",
    !is.na(PRSTDTC), PRSTDTC >= TRTSDT
  ) |>
  group_by(STUDYID, USUBJID) |>
  summarise(PRNACTDT = min(PRSTDTC), .groups = "drop")

adsl <- adsl_raw |>
  left_join(cm_min, by = c("STUDYID", "USUBJID")) |>
  left_join(pr_min, by = c("STUDYID", "USUBJID")) |>
  mutate(
    NACTDT = case_when(
      !is.na(CMNACTDT) & !is.na(PRNACTDT) ~ pmin(CMNACTDT, PRNACTDT),
      !is.na(CMNACTDT) ~ CMNACTDT,
      !is.na(PRNACTDT) ~ PRNACTDT,
      .default = as.Date(NA)
    )
  ) |>
  mutate(
    NACTDY = as.integer(NACTDT - TRTSDT) + 1L,
    NACTFL = if_else(is.na(NACTDT), NA_character_, "Y")
  ) |>
  select(STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
