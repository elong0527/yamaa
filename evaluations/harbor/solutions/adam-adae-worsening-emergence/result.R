# Reference solution for the yamaa benchmark adam-adae-worsening-emergence (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)
adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)

# Datetimes compare as text once every value carries seconds.
normalize <- function(x) {
  if_else(!is.na(x) & nchar(x) == 16L, paste0(x, ":00"), x)
}

adae <- ae |>
  left_join(adsl |> select(USUBJID, TRTSDTM), by = "USUBJID") |>
  mutate(
    ASTDTM = normalize(AESTDTC),
    TRTSDTM = normalize(TRTSDTM),
    SEVN = case_when(
      AESEV == "MILD" ~ 1L,
      AESEV == "MODERATE" ~ 2L,
      AESEV == "SEVERE" ~ 3L
    ),
    TOXN = suppressWarnings(as.integer(AETOXGR))
  ) |>
  group_by(USUBJID, AEDECOD) |>
  mutate(
    TRTEMFL = map_chr(seq_along(ASTDTM), function(i) {
      ast <- ASTDTM[i]
      trt <- TRTSDTM[i]
      if (is.na(ast) || is.na(trt)) return(NA_character_)
      if (ast >= trt) return("Y")
      # Before exposure: Y when the same term reaches a higher severity or
      # grade after exposure. A missing value on either side shows no
      # worsening for that dimension.
      after <- !is.na(ASTDTM) & ASTDTM >= trt
      sev <- any(!is.na(SEVN[after]) & !is.na(SEVN[i]) & SEVN[after] > SEVN[i])
      tox <- any(!is.na(TOXN[after]) & !is.na(TOXN[i]) & TOXN[after] > TOXN[i])
      if (isTRUE(sev) || isTRUE(tox)) "Y" else NA_character_
    })
  ) |>
  ungroup() |>
  select(
    STUDYID, USUBJID, AESEQ, AEDECOD, ASTDTM, TRTSDTM, AESEV, AETOXGR, TRTEMFL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
