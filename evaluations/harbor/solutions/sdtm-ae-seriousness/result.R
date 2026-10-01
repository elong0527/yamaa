# Reference solution for the yamaa benchmark sdtm-ae-seriousness (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

ae_raw <- read_csv(
  "/app/input/ae_raw.csv",
  col_types = cols(AESEQ = col_integer(), .default = col_character())
)

# Serious when any criterion is Y; otherwise not serious, even when every
# criterion is empty. The criterion flags stay as collected.
ae <- ae_raw |>
  mutate(
    DOMAIN = "AE",
    AESER = if_else(
      AESDTH == "Y" | AESLIFE == "Y" | AESHOSP == "Y" |
        AESDISAB == "Y" | AESCONG == "Y" | AESMIE == "Y",
      "Y", "N", missing = "N"
    )
  ) |>
  select(
    DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESER,
    AESDTH, AESLIFE, AESHOSP, AESDISAB, AESCONG, AESMIE
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ae, "/app/output/ae.csv", na = "")
