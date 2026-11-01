# Execute with Rscript /app/output/result.R.
adsl <- read.csv(
  "/app/input/adsl.csv", stringsAsFactors = FALSE, na.strings = c("", "NA"),
  colClasses = c(STUDYID = "character", USUBJID = "character", HEIGHTBL = "numeric")
)
vs <- read.csv(
  "/app/input/vs.csv", stringsAsFactors = FALSE, na.strings = c("", "NA"),
  colClasses = c(STUDYID = "character", USUBJID = "character", VSSEQ = "numeric",
                 VISITNUM = "numeric", VSDTC = "character", VSSTRESN = "numeric")
)

# Retain a dosing weight until an inclusive 10% change from that weight.
retained_weight <- function(weights) {
  anchor <- NA_real_
  result <- rep(NA_real_, length(weights))
  for (i in seq_along(weights)) {
    weight <- weights[i]
    if (!is.na(weight)) {
      if (is.na(anchor)) {
        anchor <- weight
      } else {
        # Allow only floating-point rounding error at the 10% boundary.
        tolerance <- 16 * .Machine$double.eps * max(1, abs(anchor), abs(weight))
        if (abs(weight - anchor) >= 0.10 * abs(anchor) - tolerance) {
          anchor <- weight
        }
      }
    }
    result[i] <- anchor
  }
  result
}

stopifnot(!anyDuplicated(adsl[c("STUDYID", "USUBJID")]))
stopifnot(!anyDuplicated(vs[c("STUDYID", "USUBJID", "VISITNUM")]))

vs <- merge(vs, adsl[c("STUDYID", "USUBJID", "HEIGHTBL")],
            by = c("STUDYID", "USUBJID"), all.x = TRUE, sort = FALSE)
vs <- vs[order(vs$STUDYID, vs$USUBJID, vs$VISITNUM, vs$VSSEQ), , drop = FALSE]
vs$ANCHORWT <- rep(NA_real_, nrow(vs))
if (nrow(vs) > 0L) {
  # Consecutive rows share a group only when study and subject both match.
  new_subject <- c(TRUE,
    vs$STUDYID[-1L] != vs$STUDYID[-nrow(vs)] |
    vs$USUBJID[-1L] != vs$USUBJID[-nrow(vs)])
  subject_rows <- split(seq_len(nrow(vs)), cumsum(new_subject))
  for (rows in subject_rows) {
    vs$ANCHORWT[rows] <- retained_weight(vs$VSSTRESN[rows])
  }
}

advs <- data.frame(
  STUDYID = vs$STUDYID,
  USUBJID = vs$USUBJID,
  ASEQ = vs$VSSEQ,
  AVISITN = vs$VISITNUM,
  ADT = as.Date(substr(vs$VSDTC, 1, 10)),
  AVAL = vs$VSSTRESN,
  ANCHORWT = vs$ANCHORWT,
  BSA = round(sqrt(vs$ANCHORWT * vs$HEIGHTBL / 3600), 2),
  stringsAsFactors = FALSE
)
write.csv(advs, "/app/output/advs.csv", row.names = FALSE, na = "")
