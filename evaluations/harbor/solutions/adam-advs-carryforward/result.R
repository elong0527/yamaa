# Reference solution for the yamaa benchmark adam-advs-carryforward (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

plan <- read_csv(
  "/app/input/plan.csv",
  col_types = cols(ASEQ = col_integer(), ADT = col_date(), .default = col_character())
)
vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(
    VSSEQ = col_integer(), VSDTC = col_date(), VSSTRESN = col_double(),
    .default = col_character()
  )
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
)

# The record collected for a planned measurement: same subject, test code
# as the parameter, and collection date as the planned date.
collected <- plan |>
  left_join(
    vs |> select(STUDYID, USUBJID, VSSEQ, VSTESTCD, VSDTC, AVALCOL = VSSTRESN),
    by = c("STUDYID", "USUBJID", "PARAMCD" = "VSTESTCD", "ADT" = "VSDTC")
  )

# The collected value, otherwise the most recent earlier value for the
# same subject and parameter, in planned order.
carried <- collected |>
  arrange(STUDYID, USUBJID, PARAMCD, ADT, ASEQ) |>
  group_by(STUDYID, USUBJID, PARAMCD) |>
  fill(AVALCOL, .direction = "down") |>
  mutate(AVAL = AVALCOL) |>
  ungroup()

# The latest height on or before treatment start, repeated on every
# record of the subject.
heights <- vs |>
  filter(VSTESTCD %in% "HEIGHT", !is.na(VSSTRESN)) |>
  left_join(adsl |> select(STUDYID, USUBJID, TRTSDT), by = c("STUDYID", "USUBJID")) |>
  filter(!is.na(VSDTC), VSDTC <= TRTSDT) |>
  arrange(STUDYID, USUBJID, VSDTC, VSSEQ) |>
  group_by(STUDYID, USUBJID) |>
  summarise(HEIGHTBL = last(VSSTRESN), .groups = "drop")

advs <- carried |>
  left_join(adsl |> select(STUDYID, USUBJID, TRTSDT), by = c("STUDYID", "USUBJID")) |>
  left_join(heights, by = c("STUDYID", "USUBJID")) |>
  arrange(USUBJID, ASEQ) |>
  select(STUDYID, USUBJID, ASEQ, VSSEQ, PARAMCD, ADT, AVAL, TRTSDT, HEIGHTBL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
