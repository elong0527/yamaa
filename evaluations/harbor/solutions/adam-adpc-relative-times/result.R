# Reference solution for the yamaa benchmark adam-adpc-relative-times (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(lubridate, warn.conflicts = FALSE)
library(readr)

pc <- read_csv(
  "/app/input/pc.csv",
  col_types = cols(PCSEQ = col_integer(), .default = col_character())
)
ex <- read_csv("/app/input/ex.csv", col_types = cols(.default = col_character()))

# A date without a time is not a moment; a time without seconds counts as
# second zero. Clock readings, no time zone.
moment <- function(x) {
  full <- if_else(nchar(x) == 16, paste0(x, ":00"), x)
  if_else(!is.na(x) & nchar(x) > 10, ymd_hms(full, tz = "UTC", quiet = TRUE), as.POSIXct(NA, tz = "UTC"))
}
stamp <- function(x) if_else(is.na(x), NA_character_, format(x, "%Y-%m-%dT%H:%M:%S"))

doses <- ex |>
  transmute(USUBJID, DRUG = EXTRT, START = moment(EXSTDTC)) |>
  filter(!is.na(START))

samples <- pc |>
  mutate(
    ADTMM = moment(PCDTC),
    DRUG = case_when(
      PCTESTCD %in% c("DRGX", "MET1") ~ "DRUG X",
      PCTESTCD == "DRGY" ~ "DRUG Y"
    )
  )

first <- doses |>
  group_by(USUBJID, DRUG) |>
  summarise(FIRSTM = min(START), .groups = "drop")

# The latest dose of the drug at or before the sample.
latest <- samples |>
  filter(!is.na(ADTMM)) |>
  inner_join(doses, by = c("USUBJID", "DRUG"), relationship = "many-to-many") |>
  filter(START <= ADTMM) |>
  group_by(USUBJID, PCSEQ) |>
  summarise(LATESTM = max(START), .groups = "drop")

adpc <- samples |>
  left_join(first, by = c("USUBJID", "DRUG")) |>
  left_join(latest, by = c("USUBJID", "PCSEQ")) |>
  mutate(
    # A sample taken before every dose refers to the first dose.
    REFM = if_else(is.na(ADTMM) | is.na(FIRSTM), as.POSIXct(NA, tz = "UTC"),
                   coalesce(LATESTM, FIRSTM)),
    PARAMCD = PCTESTCD,
    ATPT = PCTPT,
    ADTM = stamp(ADTMM),
    FANLDTM = stamp(FIRSTM),
    PCRFTDTM = stamp(REFM),
    AFRLT = as.numeric(difftime(ADTMM, FIRSTM, units = "hours")),
    ARRLT = as.numeric(difftime(ADTMM, REFM, units = "hours"))
  ) |>
  select(STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpc, "/app/output/adpc.csv", na = "")
