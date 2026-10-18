# Reference solution for the yamaa benchmark sdtm-suppdm-metadata (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm_raw <- read_csv(
  "/app/input/dm_raw.csv",
  col_types = cols(.default = col_character())
)

labels <- c(RACE1 = "Race 1", RACE2 = "Race 2", RACEOTH = "Race, Other")
suppdm <- bind_rows(lapply(names(labels), function(qnam) {
  dm_raw |>
    filter(!is.na(.data[[qnam]])) |>
    transmute(
      STUDYID,
      RDOMAIN = "DM",
      USUBJID,
      IDVAR = NA_character_,
      IDVARVAL = NA_character_,
      QNAM = qnam,
      QLABEL = labels[[qnam]],
      QVAL = .data[[qnam]],
      QORIG = "Collected",
      QEVAL = NA_character_
    )
}))

dir.create("/app/output", showWarnings = FALSE)
write_csv(suppdm, "/app/output/suppdm.csv", na = "")
