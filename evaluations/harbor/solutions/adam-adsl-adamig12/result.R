# Derive ADSL from ADSL_RAW using the requested ADaMIG 1.2 variables.
raw <- read.csv("/app/input/adsl_raw.csv", stringsAsFactors = FALSE,
                check.names = FALSE, na.strings = "")
required <- c("STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL", "TRTSDTC")
stopifnot(all(required %in% names(raw)))
stopifnot(!anyNA(raw$USUBJID), !anyDuplicated(raw$USUBJID))

source_datetime <- trimws(as.character(raw$TRTSDTC))
never_treated <- !is.na(raw$TRT01A) &
  toupper(trimws(raw$TRT01A)) == "NOT TREATED"
has_exposure <- !never_treated & !is.na(source_datetime) &
  nzchar(source_datetime)
trtsdtm <- rep(NA_character_, nrow(raw))

if (any(has_exposure)) {
  values <- source_datetime[has_exposure]
  valid_shape <- grepl(
    "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:[0-9]{2})?$",
    values
  )
  if (!all(valid_shape)) stop("Exposure datetime has an unsupported format")
  values[nchar(values) == 16L] <- paste0(values[nchar(values) == 16L], ":00")
  parsed <- as.POSIXct(values, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  normalized <- format(parsed, "%Y-%m-%dT%H:%M:%S", tz = "UTC")
  if (anyNA(parsed) || !all(normalized == values)) {
    stop("Invalid exposure date or time")
  }
  trtsdtm[has_exposure] <- normalized
}

adsl <- raw[c("STUDYID", "USUBJID", "TRT01A", "AGE", "HEIGHTBL")]
adsl$TRTSDT <- substr(trtsdtm, 1L, 10L)
adsl$TRTSDTM <- trtsdtm
stopifnot(nrow(adsl) == nrow(raw), !anyDuplicated(adsl$USUBJID))
stopifnot(all(is.na(adsl$TRTSDT[never_treated])),
          all(is.na(adsl$TRTSDTM[never_treated])))
dir.create("/app/output", showWarnings = FALSE)
write.csv(adsl, "/app/output/adsl.csv", row.names = FALSE, na = "")
