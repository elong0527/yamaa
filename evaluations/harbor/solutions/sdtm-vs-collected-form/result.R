# Reference solution for the yamaa benchmark sdtm-vs-collected-form (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(purrr)
library(readr)
library(tidyr)

fmt_text <- function(x) {
  if (is.na(x)) {
    return(NA_character_)
  }
  if (x == floor(x)) {
    formatC(x, format = "f", digits = 0)
  } else {
    format(x, scientific = FALSE, trim = TRUE, digits = 15)
  }
}

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)
tests <- read_csv(
  "/app/input/vs_tests.csv",
  col_types = cols(TESTORD = col_integer(), .default = col_character())
)

wide <- odm |>
  select(StudyOID, SubjectKey, StudyEventOID, ItemOID, Value) |>
  pivot_wider(names_from = ItemOID, values_from = Value, values_fn = first)

rows <- list()
for (i in seq_len(nrow(wide))) {
  visit <- wide[i, ]
  vsdtc <- visit$IT.VS.VSDTC
  position <- visit$IT.VS.POSITION
  method <- visit$IT.VS.TEMPMETHOD
  not_done <- !is.na(visit$IT.VS.BPNOTDONE) && visit$IT.VS.BPNOTDONE == "Y"
  reason <- visit$IT.VS.BPREASND
  for (j in seq_len(nrow(tests))) {
    test <- tests[j, ]
    testcd <- test$VSTESTCD
    result_item <- paste0("IT.VS.", testcd)
    unit_item <- paste0("IT.VS.", testcd, "U")
    result <- if (result_item %in% names(visit)) visit[[result_item]] else NA_character_
    unit <- if (unit_item %in% names(visit)) visit[[unit_item]] else NA_character_
    has_result <- !is.na(result) && result != ""
    if (testcd %in% c("SYSBP", "DIABP") && not_done) {
      has_result <- FALSE
    }
    if (!has_result) {
      orres <- NA_character_
      orresu <- NA_character_
      stresn <- NA_real_
      stresc <- NA_character_
      stresu <- NA_character_
      pos <- NA_character_
      meth <- NA_character_
    } else {
      orres <- result
      orresu <- unit
      stresn <- suppressWarnings(as.numeric(result))
      stresc <- fmt_text(stresn)
      stresu <- unit
      pos <- if (test$HAS_POSITION == "1") position else NA_character_
      meth <- if (test$HAS_METHOD == "1") method else NA_character_
    }
    if (test$HAS_NOTDONE == "1" && not_done) {
      stat <- "NOT DONE"
      reasnd <- reason
    } else {
      stat <- NA_character_
      reasnd <- NA_character_
    }
    rows[[length(rows) + 1]] <- tibble(
      STUDYID = visit$StudyOID,
      USUBJID = visit$SubjectKey,
      VISIT = visit$StudyEventOID,
      VSTESTCD = testcd,
      VSTEST = test$VSTEST,
      TESTORD = test$TESTORD,
      VSORRES = orres,
      VSORRESU = orresu,
      VSSTRESN = stresn,
      VSSTRESC = stresc,
      VSSTRESU = stresu,
      VSPOS = pos,
      VSMETHOD = meth,
      VSSTAT = stat,
      VSREASND = reasnd,
      VSDTC = vsdtc
    )
  }
}

vs <- bind_rows(rows) |>
  arrange(USUBJID, VSDTC, TESTORD) |>
  group_by(USUBJID) |>
  mutate(VSSEQ = row_number()) |>
  ungroup() |>
  arrange(USUBJID, VSSEQ) |>
  transmute(
    DOMAIN = "VS",
    STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
    VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
