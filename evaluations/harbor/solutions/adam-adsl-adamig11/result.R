# Create the subject-level ADaM ADSL dataset from the supplied source.
raw <- read.csv("/app/input/adsl_raw.csv", stringsAsFactors = FALSE,
                na.strings = c("", "NA"), check.names = FALSE)

required <- c("STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL", "TRTSDTC")
stopifnot(all(required %in% names(raw)), !anyNA(raw$USUBJID),
          !anyDuplicated(raw$USUBJID))

# Complete minute-resolution collection values with zero seconds.
trtsdtm <- trimws(raw$TRTSDTC)
minute_only <- !is.na(trtsdtm) &
  grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}$", trtsdtm)
trtsdtm[minute_only] <- paste0(trtsdtm[minute_only], ":00")
trtsdtm[is.na(trtsdtm) | trtsdtm == ""] <- NA_character_
trtsdt <- as.Date(substr(trtsdtm, 1L, 10L), format = "%Y-%m-%d")

adsl <- data.frame(
  STUDYID = raw$STUDYID,
  USUBJID = raw$USUBJID,
  TRT01A = raw$TRT01A,
  AGE = raw$AGE,
  HEIGHTBL = raw$HEIGHTBL,
  TRTSDT = trtsdt,
  TRTSDTM = trtsdtm,
  stringsAsFactors = FALSE
)
write.csv(adsl, "/app/output/adsl.csv", row.names = FALSE, na = "")
