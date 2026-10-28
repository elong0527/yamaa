source <- read.csv("/app/input/nca_transfer.csv", colClasses = "character", stringsAsFactors = FALSE, check.names = FALSE)
code_map <- c(Cmax = "CMAX", Tmax = "TMAX", AUClast = "AUCLST")
name_map <- c(CMAX = "Max Conc", TMAX = "Time of CMAX Observation", AUCLST = "AUC to Last Nonzero Conc")
stopifnot(all(source$Parameter %in% names(code_map)))
pp <- data.frame(
  DOMAIN = "PP",
  STUDYID = source$Protocol,
  USUBJID = source$Subject,
  PPSEQ = integer(nrow(source)),
  PPTESTCD = unname(code_map[source$Parameter]),
  PPTEST = unname(name_map[code_map[source$Parameter]]),
  PPCAT = source$Analyte,
  PPORRES = source$Estimate,
  PPORRESU = source$Unit,
  PPSTRESC = source$Estimate,
  PPSTRESN = as.numeric(source$Estimate),
  PPSTRESU = source$Unit,
  PPSPEC = "PLASMA",
  PPRFTDTC = format(as.POSIXct(source$ReferenceDose, format = "%Y-%m-%dT%H:%M:%S", tz = "UTC"), "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
  stringsAsFactors = FALSE
)
pp <- pp[order(pp$STUDYID, pp$USUBJID, pp$PPRFTDTC, pp$PPCAT, pp$PPTESTCD), ]
group <- interaction(pp$STUDYID, pp$USUBJID, drop = TRUE)
pp$PPSEQ <- as.integer(ave(seq_len(nrow(pp)), group, FUN = seq_along))
stopifnot(nrow(pp) == nrow(source), !anyNA(pp))
write.csv(pp, "/app/output/pp.csv", row.names = FALSE, na = "")
