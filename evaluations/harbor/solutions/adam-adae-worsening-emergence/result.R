# R reference for the yamaa benchmark adam-adae-worsening-emergence.
# Uses the collected event records and dated findings to construct periods.
library(readr)

read_input <- function(name) {
  read_csv(paste0('/app/input/', name),
           col_types = cols(.default = col_character()),
           show_col_types = FALSE)
}
normalize <- function(x) {
  short <- !is.na(x) & nchar(x) == 16L
  x[short] <- paste0(x[short], ':00')
  x
}
ae <- read_input('ae.csv')
fa <- read_input('fa.csv')
adsl <- read_input('adsl.csv')
fa$FADTC <- normalize(fa$FADTC)
ae <- ae[order(ae$STUDYID, ae$USUBJID, as.integer(ae$AESEQ)), ]
ranks <- c(MILD = 1L, MODERATE = 2L, SEVERE = 3L)
output <- list()
previous_study <- previous_subject <- NA_character_
aseq <- 0L
for (i in seq_len(nrow(ae))) {
  event <- ae[i, ]
  subject <- event$USUBJID
  study <- event$STUDYID
  if (!identical(study, previous_study) ||
      !identical(subject, previous_subject)) aseq <- 0L
  previous_study <- study
  previous_subject <- subject
  trt <- normalize(adsl$TRTSDTM[
    adsl$STUDYID == study & adsl$USUBJID == subject
  ])[1]
  onset <- normalize(event$AESTDTC)
  ending <- normalize(event$AEENDTC)
  findings <- fa[fa$STUDYID == study & fa$USUBJID == subject &
                   fa$FALNKID == event$AELNKID, ]
  times <- sort(unique(findings$FADTC))
  sevs <- rep(NA_character_, length(times))
  grades <- rep(NA_integer_, length(times))
  sev <- NA_character_
  grade <- NA_integer_
  for (j in seq_along(times)) {
    updates <- findings[findings$FADTC == times[j], ]
    if (anyDuplicated(updates$FATESTCD)) stop('Duplicate finding at event/time')
    new_sev <- updates$FASTRESC[updates$FATESTCD == 'SEV']
    new_grade <- updates$FASTRESC[updates$FATESTCD == 'TOXGR']
    if (length(new_sev) && !is.na(new_sev)) sev <- new_sev
    if (length(new_grade) && !is.na(new_grade)) grade <- as.integer(new_grade)
    sevs[j] <- sev
    grades[j] <- grade
  }
  before <- if (is.na(trt)) integer() else which(times < trt)
  baseline_sev <- baseline_grade <- NA_integer_
  if (length(before)) {
    last <- tail(before, 1)
    baseline_sev <- unname(ranks[sevs[last]])
    baseline_grade <- grades[last]
  }
  emergent <- FALSE
  for (j in seq_along(times)) {
    if (!is.na(onset) && !is.na(trt)) {
      if (onset >= trt) emergent <- TRUE
      else if (times[j] >= trt) {
        rank <- unname(ranks[sevs[j]])
        sev_worse <- !is.na(rank) && !is.na(baseline_sev) && rank > baseline_sev
        grade_worse <- !is.na(grades[j]) && !is.na(baseline_grade) &&
          grades[j] > baseline_grade
        emergent <- emergent || sev_worse || grade_worse
      }
    }
    aseq <- aseq + 1L
    output[[length(output) + 1L]] <- data.frame(
      STUDYID = study, USUBJID = subject, ASEQ = aseq,
      AESEQ = as.integer(event$AESEQ), AEDECOD = event$AEDECOD,
      AESTDTC = onset, AEENDTC = ending, ASTDTM = times[j],
      AENDTM = if (j < length(times)) times[j + 1L] else ending,
      TRTSDTM = trt, AESEV = event$AESEV, AETOXGR = event$AETOXGR,
      ASEV = sevs[j], ATOXGRN = grades[j],
      TRTEMFL = if (emergent) 'Y' else NA_character_
    )
  }
}
dir.create('/app/output', showWarnings = FALSE)
write_csv(do.call(rbind, output), '/app/output/adae.csv', na = '')
