# Reference solution for the yamaa benchmark adam-adae-prior-serious-event (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae <- read_csv(
  "/app/input/ae.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

serious <- ae |>
  filter(AESER %in% "Y") |>
  arrange(USUBJID, AESEQ)

# The subject's first serious event, whatever its term holds.
first_sae <- serious |>
  distinct(USUBJID, .keep_all = TRUE) |>
  select(USUBJID, FIRST_SAEDECOD = AEDECOD, FIRST_SAESEQ = AESEQ)

adae <- ae |>
  left_join(first_sae, by = "USUBJID") |>
  group_by(USUBJID) |>
  arrange(AESEQ, .by_group = TRUE) |>
  mutate(
    # The most recent earlier serious event, by sequence number.
    PRIOR_SAESEQ = sapply(
      AESEQ,
      function(s) {
        earlier <- AESEQ[AESER %in% "Y" & AESEQ < s]
        if (length(earlier) == 0L) NA_integer_ else max(earlier)
      }
    ),
    PRIOR_SAEDECOD = sapply(
      PRIOR_SAESEQ,
      function(s) {
        if (is.na(s)) NA_character_ else AEDECOD[match(s, AESEQ)]
      }
    ),
    PREV_SEQ = sapply(
      AESEQ,
      function(s) {
        earlier <- AESEQ[AESEQ < s]
        if (length(earlier) == 0L) NA_integer_ else max(earlier)
      }
    ),
    PREV_AEDECOD = sapply(
      PREV_SEQ,
      function(s) {
        if (is.na(s)) NA_character_ else AEDECOD[match(s, AESEQ)]
      }
    ),
    PREV_AEDECOD = if_else(
      PREV_AEDECOD == "" | is.na(PREV_AEDECOD),
      NA_character_,
      PREV_AEDECOD
    ),
    PRIOR_SAEDECOD = if_else(
      PRIOR_SAEDECOD == "" | is.na(PRIOR_SAEDECOD),
      NA_character_,
      PRIOR_SAEDECOD
    ),
    FIRST_SAEDECOD = if_else(
      FIRST_SAEDECOD == "" | is.na(FIRST_SAEDECOD),
      NA_character_,
      FIRST_SAEDECOD
    )
  ) |>
  ungroup() |>
  mutate(
    # A serious event leaves its own prior and first fields empty.
    PRIOR_SAEFL = if_else(
      AESER %in% "N" & !is.na(PRIOR_SAESEQ),
      "Y",
      NA_character_
    ),
    PRIOR_SAEDECOD = if_else(AESER %in% "N", PRIOR_SAEDECOD, NA_character_),
    PRIOR_SAESEQ = if_else(AESER %in% "N", PRIOR_SAESEQ, NA_integer_),
    FIRST_SAEDECOD = if_else(AESER %in% "N", FIRST_SAEDECOD, NA_character_),
    FIRST_SAESEQ = if_else(AESER %in% "N", FIRST_SAESEQ, NA_integer_)
  ) |>
  select(
    STUDYID, USUBJID, AESEQ, AEDECOD, AESER, PRIOR_SAEFL, PRIOR_SAEDECOD,
    PRIOR_SAESEQ, FIRST_SAEDECOD, FIRST_SAESEQ, PREV_AEDECOD
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adae, "/app/output/adae.csv", na = "")
