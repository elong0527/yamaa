# Create the ALT analysis dataset from the supplied subject and laboratory data.
options(stringsAsFactors = FALSE)
adsl <- read.csv("/app/input/adsl.csv", na.strings = c("", "NA"), check.names = FALSE)
lb <- read.csv("/app/input/lb.csv", na.strings = c("", "NA"), check.names = FALSE)
lb <- lb[lb$LBTESTCD == "ALT" & !is.na(lb$LBTESTCD), , drop = FALSE]

subject_key <- paste(adsl$STUDYID, adsl$USUBJID, sep = "\r")
stopifnot(!anyDuplicated(subject_key))
subject_match <- match(paste(lb$STUDYID, lb$USUBJID, sep = "\r"), subject_key)
stopifnot(!anyNA(subject_match))

adt <- as.Date(substr(lb$LBDTC, 1L, 10L))
trtsdt <- as.Date(adsl$TRTSDT[subject_match])
day_difference <- as.integer(adt - trtsdt)
ady <- ifelse(is.na(day_difference), NA_integer_,
              ifelse(day_difference >= 0L, day_difference + 1L, day_difference))
aval <- as.numeric(lb$LBSTRESN)
ablfl <- ifelse(!is.na(lb$LBBLFL) & lb$LBBLFL == "Y", "Y", "")

out <- data.frame(
  STUDYID = lb$STUDYID,
  USUBJID = lb$USUBJID,
  PARAMCD = lb$LBTESTCD,
  PARAM = "Alanine Aminotransferase",
  ADT = adt,
  TRT01A = adsl$TRT01A[subject_match],
  ADY = as.integer(ady),
  AVAL = aval,
  AVALU = lb$LBSTRESU,
  ABLFL = ablfl,
  BASE = NA_real_,
  CHG = NA_real_,
  check.names = FALSE
)
parameter_key <- paste(out$STUDYID, out$USUBJID, out$PARAMCD, sep = "\r")
for (key in unique(parameter_key)) {
  rows <- which(parameter_key == key)
  baseline_rows <- rows[out$ABLFL[rows] == "Y"]
  stopifnot(length(baseline_rows) <= 1L)
  if (length(baseline_rows) == 1L) {
    out$BASE[rows] <- out$AVAL[baseline_rows]
  }
}
out$CHG <- out$AVAL - out$BASE
out <- out[order(out$STUDYID, out$USUBJID, out$PARAMCD, out$ADT), , drop = FALSE]
stopifnot(!anyDuplicated(out[c("STUDYID", "USUBJID", "PARAMCD", "ADT")]))
write.csv(out, "/app/output/adlb.csv", row.names = FALSE, na = "")
