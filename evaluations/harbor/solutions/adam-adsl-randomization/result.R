# Reference solution for the yamaa benchmark adam-adsl-randomization (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- nanoparquet::read_parquet("/app/input/dm.parquet")

# Study day from the reference start: the reference day itself is day 1,
# later dates count forward inclusively, earlier dates count backward with
# no day 0, so the day before is day -1.
adsl <- dm |>
  mutate(
    .diff = as.integer(RANDDT - RFSTDTC),
    RANDDY = case_when(
      is.na(.diff) ~ NA_integer_,
      .diff >= 0L ~ .diff + 1L,
      .default = .diff
    ),
    # The collected moment as ISO 8601 text with a T, at whole-second
    # precision, written from what was collected even when no date was.
    RANDDTC = format(RANDDTTM, "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  ) |>
  select(STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
