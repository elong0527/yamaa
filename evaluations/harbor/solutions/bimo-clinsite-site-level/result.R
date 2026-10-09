# Reference solution for the yamaa benchmark bimo-clinsite-site-level (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

subj <- read_csv(
  "/app/input/subjects.csv",
  col_types = cols(.default = col_character())
)

clinsite <- subj |>
  # One record per site; the site key stays text so leading zeros survive.
  group_by(STUDYID, SITENUM) |>
  summarise(
    # The alphabetically first arm among the site's treated subjects.
    ARM = {
      treated_arms <- ARM[SAFFL == "Y"]
      if (length(treated_arms)) min(treated_arms) else NA_character_
    },
    # Treated subjects, then all enrolled subjects, at the site.
    SAFPOP = sum(SAFFL == "Y"),
    ENRLPOP = n(),
    .groups = "drop"
  ) |>
  rename(SITEID = SITENUM) |>
  select(STUDYID, SITEID, ARM, SAFPOP, ENRLPOP) |>
  arrange(SITEID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(clinsite, "/app/output/clinsite.csv", na = "")
