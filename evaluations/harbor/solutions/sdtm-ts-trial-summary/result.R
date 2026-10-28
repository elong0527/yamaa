protocol <- read.csv("/app/input/protocol.csv", stringsAsFactors = FALSE, check.names = FALSE)
rows <- lapply(seq_len(nrow(protocol)), function(i) {
  p <- protocol[i, ]
  data.frame(
    DOMAIN = rep("TS", 4),
    STUDYID = rep(p$Study, 4),
    TSSEQ = 1:4,
    TSPARMCD = c("TITLE", "TPHASE", "RANDOM", "PLANSUB"),
    TSPARM = c("Trial Title", "Trial Phase Classification", "Trial is Randomized", "Planned Number of Subjects"),
    TSVAL = c(p$Title,
              if (p$Phase == "Phase 2") "PHASE II TRIAL" else NA_character_,
              if (p$Randomized == "Yes") "Y" else NA_character_,
              as.character(as.integer(p$PlannedSubjects))),
    TSVALCD = c(NA_character_, if (p$Phase == "Phase 2") "C15601" else NA_character_,
                if (p$Randomized == "Yes") "C49488" else NA_character_, NA_character_),
    TSVCDREF = c(NA_character_, "CDISC", "CDISC", NA_character_),
    stringsAsFactors = FALSE
  )
})
ts <- do.call(rbind, rows)
write.csv(ts, "/app/output/ts.csv", row.names = FALSE, na = "")
stopifnot(identical(names(ts), c("DOMAIN", "STUDYID", "TSSEQ", "TSPARMCD", "TSPARM", "TSVAL", "TSVALCD", "TSVCDREF")),
          nrow(ts) == 4L * nrow(protocol))
