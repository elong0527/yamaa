subjects <- read.csv("/app/input/adsl.csv", stringsAsFactors = FALSE)
events <- read.csv("/app/input/adae.csv", stringsAsFactors = FALSE)
events$ASTDT <- as.Date(events$ASTDT)
risk <- do.call(rbind, lapply(seq_len(nrow(subjects)), function(i) {
  subject <- subjects[i, ]
  start <- as.Date(subject$TRTSDT)
  end <- as.Date(subject$TRTEDT) + 30
  do.call(rbind, lapply(c("DIARRHOEA", "HEADACHE"), function(term) {
    selected <- events$STUDYID == subject$STUDYID &
      events$USUBJID == subject$USUBJID & events$AEDECOD == term &
      !is.na(events$ASTDT) & events$ASTDT >= start & events$ASTDT <= end
    onsets <- events$ASTDT[selected]
    first <- if (length(onsets)) min(onsets) else as.Date(NA)
    endpoint <- if (length(onsets)) first else end
    data.frame(STUDYID = subject$STUDYID, USUBJID = subject$USUBJID,
      TRT01A = subject$TRT01A, AEDECOD = term, FIRSTDT = first,
      EVENTN = as.integer(length(onsets) > 0),
      RISKDAYS = as.integer(endpoint - start) + 1L)
  }))
}))
groups <- split(risk, interaction(risk$STUDYID, risk$TRT01A, risk$AEDECOD,
                                  drop = TRUE))
rates <- do.call(rbind, lapply(groups, function(group) {
  count <- sum(group$EVENTN)
  days <- sum(group$RISKDAYS)
  years <- days / 365.25
  data.frame(STUDYID = group$STUDYID[1], TRT01A = group$TRT01A[1],
    AEDECOD = group$AEDECOD[1], NSUBJ = count, PERSONDAYS = days,
    PERSONYRS = years, EAIR100 = 100 * count / years)
}))
risk <- risk[order(risk$STUDYID, risk$USUBJID, risk$AEDECOD), ]
rates <- rates[order(rates$STUDYID, rates$TRT01A, rates$AEDECOD), ]
write.csv(risk, "/app/output/adrisk.csv", row.names = FALSE, na = "")
write.csv(rates, "/app/output/adinc.csv", row.names = FALSE, na = "")
