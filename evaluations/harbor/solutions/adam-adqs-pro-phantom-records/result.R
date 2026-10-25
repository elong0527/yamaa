# Reference solution for the yamaa benchmark adam-adqs-pro-phantom-records (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages(library(readr))

in_dir <- "/app/input"
out_dir <- "/app/output"
dir.create(out_dir, showWarnings = FALSE, recursive = TRUE)

read_chr <- function(name) {
  d <- as.data.frame(read_csv(file.path(in_dir, name),
                              col_types = cols(.default = col_character()),
                              na = character(), trim_ws = TRUE))
  d[] <- lapply(d, function(x) ifelse(is.na(x) | trimws(x) == "", NA_character_, x))
  d
}

adsl  <- read_chr("adsl.csv")
qs    <- read_chr("qs.csv")
sched <- read_chr("schedule.csv")

sched$VISITNUM  <- as.numeric(sched$VISITNUM)
sched$VISITDY   <- as.numeric(sched$VISITDY)
sched$sched_ord <- seq_len(nrow(sched))
qs$VISITNUM <- as.numeric(qs$VISITNUM)
qs$QSSTRESN <- as.numeric(qs$QSSTRESN)
qs$QSSEQ    <- as.numeric(qs$QSSEQ)

items <- unique(sched$PARAMCD[sched$PARCAT2 == "ITEM"])
tot_cd <- unique(sched$PARAMCD[sched$PARCAT2 == "TOTAL SCORE"])

# Event study days with RANDDT as day 1
randdt <- as.Date(adsl$RANDDT)
adsl$EOTDY <- as.numeric(as.Date(adsl$EOTDT) - randdt) + 1
adsl$DTHDY <- as.numeric(as.Date(adsl$DTHDT) - randdt) + 1

# Frame: every ADSL subject x every planned parameter/visit
subj <- adsl[, c("USUBJID", "SAFFL", "DCTREAS", "EOTDY", "DTHDY")]
fr <- merge(subj, sched, by = NULL)
fr$after_eot <- !is.na(fr$EOTDY) & fr$VISITDY > fr$EOTDY
fr$after_dth <- !is.na(fr$DTHDY) & fr$VISITDY > fr$DTHDY

# Collected item records: one per subject/visit/item (first by QSSEQ)
qi <- qs[qs$QSTESTCD %in% items, ]
qi <- qi[order(qi$USUBJID, qi$VISITNUM, qi$QSTESTCD, qi$QSSEQ), ]
qi <- qi[!duplicated(qi[, c("USUBJID", "VISITNUM", "QSTESTCD")]), ]
qi <- data.frame(USUBJID = qi$USUBJID, VISITNUM = qi$VISITNUM, PARAMCD = qi$QSTESTCD,
                 QSSTRESN = qi$QSSTRESN, QS_STAT = qi$QSSTAT, QS_REASND = qi$QSREASND,
                 collected = TRUE, stringsAsFactors = FALSE)

# Total score summary per subject/visit
keys <- unique(qi[, c("USUBJID", "VISITNUM")])
tot <- do.call(rbind, lapply(seq_len(nrow(keys)), function(i) {
  g <- qi[qi$USUBJID == keys$USUBJID[i] & qi$VISITNUM == keys$VISITNUM[i], ]
  g <- g[match(items, g$PARAMCD), ]
  answered <- !is.na(g$PARAMCD) & !is.na(g$QSSTRESN)
  r <- unique(g$QS_REASND[!is.na(g$QS_REASND)])
  data.frame(USUBJID = keys$USUBJID[i], VISITNUM = keys$VISITNUM[i],
             n_coll = sum(!is.na(g$PARAMCD)),
             tot_val = if (all(answered)) sum(g$QSSTRESN) else NA_real_,
             tot_reasnd = if (length(r) == 0) NA_character_ else r[1],
             stringsAsFactors = FALSE)
}))

d <- merge(fr, qi, by = c("USUBJID", "VISITNUM", "PARAMCD"), all.x = TRUE)
d <- merge(d, tot, by = c("USUBJID", "VISITNUM"), all.x = TRUE)
d$collected[is.na(d$collected)] <- FALSE
d$n_coll[is.na(d$n_coll)] <- 0

is_tot  <- d$PARAMCD %in% tot_cd
phantom <- ifelse(is_tot, d$n_coll == 0, !d$collected)

aval <- ifelse(is_tot, d$tot_val, d$QSSTRESN)
aval[phantom] <- NA_real_

qsstat   <- ifelse(phantom | is_tot, NA_character_, d$QS_STAT)
qsreasnd <- ifelse(phantom | is_tot, NA_character_, d$QS_REASND)

areasnd <- rep(NA_character_, nrow(d))
need <- is.na(aval)
ph_nt  <- need & phantom & d$SAFFL %in% "N"
ph_eot <- need & phantom & !ph_nt & d$after_eot
areasnd[ph_nt]  <- "NOT TREATED"
areasnd[ph_eot] <- d$DCTREAS[ph_eot]
ci <- need & !phantom & !is_tot
areasnd[ci] <- d$QS_REASND[ci]
ct <- need & !phantom & is_tot
areasnd[ct] <- ifelse(is.na(d$tot_reasnd[ct]), "NOT CALCULABLE", d$tot_reasnd[ct])

out <- data.frame(
  STUDYID  = d$STUDYID,
  USUBJID  = d$USUBJID,
  PARCAT2  = d$PARCAT2,
  PARAMCD  = d$PARAMCD,
  PARAM    = d$PARAM,
  AVISIT   = d$VISIT,
  AVISITN  = d$VISITNUM,
  AVAL     = aval,
  DTYPE    = ifelse(phantom, "PHANTOM", NA_character_),
  QSSTAT   = qsstat,
  QSREASND = qsreasnd,
  AREASND  = areasnd,
  PROEX1FL = ifelse(!d$after_dth, "Y", NA_character_),
  PROEX2FL = ifelse(d$SAFFL %in% "Y" & !d$after_eot & !d$after_dth, "Y", NA_character_),
  PROSCMFL = ifelse(!is.na(aval), "Y", NA_character_),
  stringsAsFactors = FALSE
)
out <- out[order(out$USUBJID, out$AVISITN, d$sched_ord), ]
rownames(out) <- NULL

write_csv(out, file.path(out_dir, "adqs.csv"), na = "")
