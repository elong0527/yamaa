# Reference solution for the yamaa benchmark sdtm-cm-collected-medications (R track).
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

# A plain number carries the dose; anything else (like a range 50-75) stays
# as text. Frequency and route labels map to controlled terminology.
cm <- odm |>
  filter(ItemGroupOID == "IG.CM") |>
  pivot_wider(
    id_cols = c(StudyOID, SubjectKey, ItemGroupRepeatKey),
    names_from = ItemOID,
    values_from = Value,
    values_fn = first
  ) |>
  mutate(
    CMSEQ = as.integer(ItemGroupRepeatKey),
    DOSE_RAW = str_trim(`IT.CM.CMDSTXT`),
    DOSE_PLAIN = str_detect(DOSE_RAW, "^\\d+(\\.\\d+)?$"),
    # BEFORE/SCREENING when the taken-before-study box is checked; ONGOING/
    # END OF STUDY when the ongoing box is checked. An ongoing medication
    # has no end date.
    DOMAIN = "CM",
    STUDYID = StudyOID,
    USUBJID = SubjectKey,
    CMTRT = na_if(`IT.CM.CMTRT`, ""),
    CMINDC = na_if(`IT.CM.CMINDC`, ""),
    CMDOSE = if_else(
      DOSE_PLAIN, suppressWarnings(as.numeric(DOSE_RAW)), NA_real_,
      missing = NA_real_
    ),
    CMDOSTXT = if_else(
      DOSE_PLAIN, NA_character_, na_if(DOSE_RAW, ""), missing = na_if(DOSE_RAW, "")
    ),
    CMDOSU = na_if(`IT.CM.CMDOSU`, ""),
    CMDOSFRQ = case_when(
      `IT.CM.CMDOSFRQ` == "Once daily" ~ "QD",
      `IT.CM.CMDOSFRQ` == "Twice daily" ~ "BID",
      .default = na_if(`IT.CM.CMDOSFRQ`, "")
    ),
    CMROUTE = case_when(
      `IT.CM.CMROUTE` == "By mouth" ~ "ORAL",
      .default = na_if(`IT.CM.CMROUTE`, "")
    ),
    CMSTDTC = na_if(`IT.CM.CMSTDTC`, ""),
    CMENDTC = if_else(
      `IT.CM.CMONGO` == "Y", NA_character_, na_if(`IT.CM.CMENDTC`, ""),
      missing = na_if(`IT.CM.CMENDTC`, "")
    ),
    CMSTRTPT = if_else(`IT.CM.CMPRIOR` == "Y", "BEFORE", NA_character_, missing = NA_character_),
    CMSTTPT = if_else(`IT.CM.CMPRIOR` == "Y", "SCREENING", NA_character_, missing = NA_character_),
    CMENRTPT = if_else(`IT.CM.CMONGO` == "Y", "ONGOING", NA_character_, missing = NA_character_),
    CMENTPT = if_else(
      `IT.CM.CMONGO` == "Y", "END OF STUDY", NA_character_, missing = NA_character_
    )
  ) |>
  arrange(USUBJID, CMSEQ) |>
  select(
    DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT,
    CMDOSU, CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT,
    CMENRTPT, CMENTPT
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(cm, "/app/output/cm.csv", na = "")
