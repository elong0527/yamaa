# Reference solution for the yamaa benchmark sdtm-ae-partial-dates (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)
library(stringr)

odm <- read_csv(
  "/app/input/odm.csv",
  col_types = cols(.default = col_character())
)
dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)

compose_dtc <- function(yr, mo, dy) {
  ifelse(
    is.na(yr) | yr == "", NA_character_,
    ifelse(
      is.na(mo) | mo == "", yr,
      ifelse(
        is.na(dy) | dy == "", paste0(yr, "-", str_pad(mo, 2, pad = "0")),
        paste0(yr, "-", str_pad(mo, 2, pad = "0"), "-", str_pad(dy, 2, pad = "0"))
      )
    )
  )
}

study_day <- function(adt, ref) {
  diff <- as.integer(as.Date(adt) - as.Date(ref))
  ifelse(is.na(diff), NA_integer_, ifelse(diff >= 0L, diff + 1L, diff))
}

# One record per form repeat; the repeat number is the sequence number.
# Dates stay at the precision collected: nothing is imputed.
ae <- odm |>
  filter(ItemGroupOID == "IG.AE") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, FormRepeatKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    AESEQ = as.integer(FormRepeatKey),
    AETERM = `IT.AE.AETERM`,
    AESTDTC = compose_dtc(`IT.AE.AESTYR`, `IT.AE.AESTMO`, `IT.AE.AESTDY`),
    AEENDTC = compose_dtc(`IT.AE.AEENYR`, `IT.AE.AEENMO`, `IT.AE.AEENDY`),
    START_FULL = !is.na(`IT.AE.AESTYR`) & `IT.AE.AESTYR` != "" &
      !is.na(`IT.AE.AESTMO`) & `IT.AE.AESTMO` != "" &
      !is.na(`IT.AE.AESTDY`) & `IT.AE.AESTDY` != "",
    END_FULL = !is.na(`IT.AE.AEENYR`) & `IT.AE.AEENYR` != "" &
      !is.na(`IT.AE.AEENMO`) & `IT.AE.AEENMO` != "" &
      !is.na(`IT.AE.AEENDY`) & `IT.AE.AEENDY` != ""
  ) |>
  left_join(
    dm |> select(USUBJID, RFSTDTC),
    by = c("SubjectKey" = "USUBJID")
  ) |>
  # Only a complete date gets a study day, and only when the reference
  # start date is known. Day 1 is the reference date; there is no day 0.
  mutate(
    AESTDY = if_else(
      START_FULL & !is.na(RFSTDTC) & RFSTDTC != "",
      study_day(AESTDTC, RFSTDTC), NA_integer_
    ),
    AEENDY = if_else(
      END_FULL & !is.na(RFSTDTC) & RFSTDTC != "",
      study_day(AEENDTC, RFSTDTC), NA_integer_
    ),
    DOMAIN = "AE",
    STUDYID = StudyOID,
    USUBJID = SubjectKey
  ) |>
  arrange(USUBJID, AESEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, AESEQ, AETERM,
    AESTDTC, AEENDTC, AESTDY, AEENDY
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(ae, "/app/output/ae.csv", na = "")
