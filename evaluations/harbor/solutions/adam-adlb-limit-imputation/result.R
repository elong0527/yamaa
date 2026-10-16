# Reference solution for the yamaa benchmark adam-adlb-limit-imputation (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    LBSEQ = col_integer(),
    .default = col_character()
  ),
  na = ""
)

# The numeric boundary of the reported result: the decimal number after an
# optional "<", ">", "<=", ">=" or "=" prefix. It has no value when no
# result was reported.
based <- lb |>
  mutate(
    PARAMCD = LBTESTCD,
    AVALC = LBSTRESC,
    AVALU = LBSTRESU,
    LIMITVAL = as.numeric(sub("^[<>=]+", "", AVALC))
  )

# Only a strict inequality receives the one-unit offset; an inclusive
# limit, an ordinary number and an "=" result keep the number itself.
adlb <- based |>
  mutate(
    AVAL = case_when(
      !is.na(AVALC) & startsWith(AVALC, "<") & !startsWith(AVALC, "<=") ~
        LIMITVAL - 1,
      !is.na(AVALC) & startsWith(AVALC, ">") & !startsWith(AVALC, ">=") ~
        LIMITVAL + 1,
      .default = LIMITVAL
    ),
    DTYPE = if_else(
      !is.na(AVALC) & (startsWith(AVALC, "<") | startsWith(AVALC, ">")),
      "IMPUTATION",
      NA_character_
    )
  ) |>
  select(
    STUDYID, USUBJID, LBSEQ, PARAMCD, AVALC, AVAL, AVALU, DTYPE
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
