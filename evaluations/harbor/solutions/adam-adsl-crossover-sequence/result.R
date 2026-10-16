# Reference solution for the yamaa benchmark
# adam-adsl-crossover-sequence (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)
ta <- read_csv(
  "/app/input/ta.csv",
  col_types = cols(TAETORD = col_integer(), .default = col_character())
)
te <- read_csv(
  "/app/input/te.csv",
  col_types = cols(.default = col_character())
)

# The treatment elements of each arm, numbered in planned order: only the
# elements in an epoch whose name begins with TREATMENT are counted.
ranked <- ta |>
  filter(startsWith(EPOCH, "TREATMENT")) |>
  arrange(STUDYID, ARMCD, TAETORD) |>
  group_by(STUDYID, ARMCD) |>
  mutate(TRTNUM = row_number()) |>
  ungroup() |>
  left_join(
    te |> select(STUDYID, ETCD, ELEMENT),
    by = c("STUDYID", "ETCD")
  )

period1 <- ranked |>
  filter(TRTNUM == 1L) |>
  transmute(STUDYID, ARMCD, TRT01P = ELEMENT)
period2 <- ranked |>
  filter(TRTNUM == 2L) |>
  transmute(STUDYID, ARMCD, TRT02P = ELEMENT)

adsl <- dm |>
  left_join(period1, by = c("STUDYID", "ARMCD")) |>
  left_join(period2, by = c("STUDYID", "ARMCD")) |>
  mutate(
    TRTSEQP = if_else(
      !is.na(TRT01P) & !is.na(TRT02P),
      paste(TRT01P, TRT02P, sep = "/"),
      NA_character_
    )
  ) |>
  select(STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
