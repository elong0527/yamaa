# Reference solution for the yamaa benchmark adam-adoe-study-eye (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
oe <- read_csv(
  "/app/input/oe.csv",
  col_types = cols(
    OESEQ = col_integer(),
    OESTRESN = col_double(),
    .default = col_character()
  )
)

# The assigned eye belongs to the subject, so the join is by subject; a
# subject without an assigned eye keeps its measurements with no role.
adoe <- oe |>
  left_join(select(adsl, USUBJID, STUDYEYE), by = "USUBJID") |>
  mutate(
    PARAMCD = OETESTCD,
    AVAL = OESTRESN,
    AFEYE = case_when(
      is.na(STUDYEYE) | STUDYEYE == "" ~ NA_character_,
      is.na(OELAT) | OELAT == "" ~ NA_character_,
      OELAT == "BILATERAL" ~ "Both Eyes",
      STUDYEYE == "BILATERAL" ~ "Study Eye",
      OELAT == STUDYEYE ~ "Study Eye",
      .default = "Fellow Eye"
    )
  ) |>
  select(STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE) |>
  arrange(USUBJID, OESEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adoe, "/app/output/adoe.csv", na = "")
