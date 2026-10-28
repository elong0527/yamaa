# Reference solution for the yamaa benchmark adam-adlb-nonparametric-limits
# (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

adsl <- read.csv("/app/input/adsl.csv", na.strings = "",
                 stringsAsFactors = FALSE)
lb <- read.csv(
  "/app/input/lb.csv",
  colClasses = c(LBSTRESN = "numeric"),
  na.strings = "",
  stringsAsFactors = FALSE
)

adlb <- merge(lb, adsl[, c("STUDYID", "USUBJID", "SEX")],
              by = c("STUDYID", "USUBJID"), all.x = TRUE, sort = FALSE)
adlb <- adlb[order(adlb$STUDYID, adlb$USUBJID, adlb$LBTESTCD), ]

# Each parameter and sex's 2.5th and 97.5th percentiles (R's default, type 7)
# and sample variance, a test not done left out.
group <- interaction(adlb$LBTESTCD, adlb$SEX, drop = TRUE)
percentile <- function(p) {
  function(x) {
    if (all(is.na(x))) return(NA_real_)
    unname(quantile(x, p, na.rm = TRUE, type = 7))
  }
}

out <- data.frame(
  STUDYID = adlb$STUDYID,
  USUBJID = adlb$USUBJID,
  SEX = adlb$SEX,
  PARAMCD = adlb$LBTESTCD,
  AVAL = adlb$LBSTRESN,
  ANRLO = ave(adlb$LBSTRESN, group, FUN = percentile(0.025)),
  ANRHI = ave(adlb$LBSTRESN, group, FUN = percentile(0.975)),
  AVALVAR = ave(adlb$LBSTRESN, group, FUN = function(x) var(x, na.rm = TRUE))
)

dir.create("/app/output", showWarnings = FALSE)
write.csv(out, "/app/output/adlb.csv", row.names = FALSE, na = "")
