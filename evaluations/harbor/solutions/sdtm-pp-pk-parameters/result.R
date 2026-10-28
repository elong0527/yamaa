source <- read.csv("/app/input/nca_transfer.csv", colClasses = "character", check.names = FALSE, stringsAsFactors = FALSE)
parameter_codes <- c(Cmax = "CMAX", Tmax = "TMAX", AUClast = "AUCLST")
parameter_names <- c(Cmax = "Max Conc", Tmax = "Time of CMAX Observation", AUClast = "AUC to Last Nonzero Conc")
parameter_units <- c(Cmax = "ng/mL", Tmax = "h", AUClast = "h*ng/mL")
stopifnot(all(source$Parameter %in% names(parameter_codes)))
pp <- data.frame(
  DOMAIN = "PP",
  STUDYID = source$Protocol,
  USUBJID = source$Subject,
  PPSEQ = 0L,
  PPTESTCD = unname(parameter_codes[source$Parameter]),
  PPTEST = unname(parameter_names[source$Parameter]),
  PPCAT = source$Analyte,
  PPORRES = source$Estimate,
  PPORRESU = unname(parameter_units[source$Parameter]),
  PPSTRESC = source$Estimate,
  PPSTRESN = as.numeric(source$Estimate),
  PPSTRESU = unname(parameter_units[source$Parameter]),
  PPSPEC = "PLASMA",
  PPRFTDTC = source$ReferenceDose,
  stringsAsFactors = FALSE
)
pp <- pp[order(pp$STUDYID, pp$USUBJID, pp$PPRFTDTC, pp$PPCAT, pp$PPTESTCD), ]
pp$PPSEQ <- as.integer(ave(seq_len(nrow(pp)), pp$STUDYID, pp$USUBJID, FUN = seq_along))
write.csv(pp, "/app/output/pp.csv", row.names = FALSE, na = "")
