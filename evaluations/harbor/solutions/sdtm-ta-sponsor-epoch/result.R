# Reference solution for the yamaa benchmark sdtm-ta-sponsor-epoch (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

design <- read_csv(
  "/app/input/design.csv",
  col_types = cols(.default = col_character())
)

ta <- design |>
  mutate(
    DOMAIN = "TA",
    TAETORD = as.integer(TAETORD)
  ) |>
  select(DOMAIN, STUDYID, ARMCD, ARM, TAETORD, ETCD, ELEMENT, EPOCH)

dir.create("/app/output", showWarnings = FALSE)
write_csv(ta, "/app/output/ta.csv", na = "")
