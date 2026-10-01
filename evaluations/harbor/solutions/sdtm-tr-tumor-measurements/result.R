# Reference solution for the yamaa benchmark sdtm-tr-tumor-measurements (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

fmt_num <- function(x) {
  if (is.na(x)) {
    return(NA_character_)
  }
  if (x == floor(x)) {
    formatC(x, format = "f", digits = 0)
  } else {
    formatted <- format(x, scientific = FALSE, trim = TRUE, digits = 15)
    sub("\\.0+$", "", formatted)
  }
}

raw <- read_csv(
  "/app/input/tr_raw.csv",
  col_types = cols(
    TRSEQ = col_integer(),
    VISITNUM = col_integer(),
    .default = col_character()
  )
)

tr <- raw |>
  mutate(
    orres_num = suppressWarnings(as.numeric(TRORRES)),
    mm_value = if_else(TRORRESU == "cm", orres_num * 10, orres_num),
    TRSTRESN = case_when(
      TRTESTCD == "DIAMETER" & TRORRES == "TOO SMALL TO MEASURE" ~ 5,
      TRTESTCD == "DIAMETER" & !is.na(TRORRES) & TRORRES != "" &
        TRORRES != "TOO SMALL TO MEASURE" ~ mm_value,
      .default = NA_real_
    ),
    TRSTRESC = case_when(
      TRTESTCD == "DIAMETER" & TRORRES == "TOO SMALL TO MEASURE" ~ "5",
      TRTESTCD == "DIAMETER" & !is.na(TRORRES) & TRORRES != "" &
        TRORRES != "TOO SMALL TO MEASURE" ~ vapply(
        TRSTRESN, fmt_num, character(1)
      ),
      TRTESTCD != "DIAMETER" & !is.na(TRORRES) & TRORRES != "" ~ TRORRES,
      .default = NA_character_
    ),
    DOMAIN = "TR",
    TRSTRESU = if_else(!is.na(TRSTRESN), "mm", NA_character_)
  ) |>
  arrange(USUBJID, TRSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES,
    TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL,
    VISITNUM, TRDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(tr, "/app/output/tr.csv", na = "")
