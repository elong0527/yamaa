# Reference solution for the yamaa benchmark adam-adpp-comparison-sets (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adpp_raw.csv",
  col_types = cols(
    APERIOD = col_integer(),
    AVAL = col_double(),
    .default = col_character()
  )
)
adsl <- read_csv("/app/input/adsl.csv", col_types = cols(.default = col_character()))
compare <- read_csv(
  "/app/input/compare.csv",
  col_types = cols(COMPNO = col_integer(), .default = col_character())
)

keys <- c("STUDYID", "USUBJID", "PARAMCD")
stopifnot(!anyDuplicated(raw[c(keys, "TRTA")]))

adpp <- raw |>
  left_join(select(adsl, STUDYID, USUBJID, TRTSEQP), by = c("STUDYID", "USUBJID"))

# Flag nn belongs to comparison nn. A record is used when its treatment is
# one of the two, its own value is evaluable (present and > 0), and the same
# subject has an evaluable value of the same parameter under the other one.
partner <- raw |> transmute(STUDYID, USUBJID, PARAMCD, OTHER = TRTA, PAVAL = AVAL)
for (number in 1:2) {
  row <- filter(compare, COMPNO == number)
  stopifnot(nrow(row) == 1)
  test <- row$TESTTRT
  ref <- row$REFTRT
  flag <- sprintf("ANL%02dFL", number)
  adpp <- adpp |>
    mutate(OTHER = case_when(TRTA == test ~ ref, TRTA == ref ~ test)) |>
    left_join(partner, by = c(keys, "OTHER")) |>
    mutate(
      "{flag}" := if_else(
        !is.na(OTHER) & !is.na(AVAL) & AVAL > 0 & !is.na(PAVAL) & PAVAL > 0,
        "Y", NA_character_
      )
    ) |>
    select(-OTHER, -PAVAL)
}

adpp <- adpp |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL, ANL01FL, ANL02FL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
