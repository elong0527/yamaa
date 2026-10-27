# Derive ADaM adverse-event start date/time and time-imputation flags.
ae <- read.csv("/app/input/ae.csv", stringsAsFactors = FALSE, check.names = FALSE,
               na.strings = c("", "NA"))
required <- c("STUDYID", "USUBJID", "AESEQ", "AETERM", "AESTDTC")
stopifnot(all(required %in% names(ae)))

n <- nrow(ae)
adae <- ae[c("STUDYID", "USUBJID", "AESEQ", "AETERM")]
adae$ASTDTM <- rep(NA_character_, n)
adae$ASTDT <- rep(NA_character_, n)
adae$ASTTM <- rep(NA_integer_, n)
adae$ASTTMF <- rep(NA_character_, n)

for (i in seq_len(n)) {
  dtc <- ae$AESTDTC[i]
  if (is.na(dtc)) next
  # Each accepted form must contain a complete calendar date.
  if (!grepl("^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}(:[0-9]{2}(:[0-9]{2})?)?)?$", dtc)) next
  date_text <- substr(dtc, 1L, 10L)
  parsed_date <- as.Date(date_text, format = "%Y-%m-%d")
  if (is.na(parsed_date) || format(parsed_date, "%Y-%m-%d") != date_text) next
  precision <- nchar(dtc)
  hour <- if (precision >= 13L) as.integer(substr(dtc, 12L, 13L)) else 0L
  minute <- if (precision >= 16L) as.integer(substr(dtc, 15L, 16L)) else 0L
  second <- if (precision >= 19L) as.integer(substr(dtc, 18L, 19L)) else 0L
  if (hour > 23L || minute > 59L || second > 59L) next
  adae$ASTDTM[i] <- sprintf("%sT%02d:%02d:%02d", date_text, hour, minute, second)
  adae$ASTDT[i] <- date_text
  adae$ASTTM[i] <- hour * 3600L + minute * 60L + second
  adae$ASTTMF[i] <- switch(as.character(precision), `10` = "H", `13` = "M",
                         `16` = "S", NA_character_)
}

stopifnot(nrow(adae) == nrow(ae),
          identical(names(adae), c("STUDYID", "USUBJID", "AESEQ", "AETERM",
                                   "ASTDTM", "ASTDT", "ASTTM", "ASTTMF")))
dir.create("/app/output", showWarnings = FALSE)
write.csv(adae, "/app/output/adae.csv", row.names = FALSE, na = "")
