# Reference solution for the yamaa benchmark adam-adlb-reference-limits (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

lb <- read.csv(
  "/app/input/lb.csv",
  colClasses = c(LBSTRESN = "numeric"),
  na.strings = "",
  stringsAsFactors = FALSE
)

# Each parameter's 95% reference range from the study's results: the mean
# less and plus the standard normal 0.975 quantile times the sample standard
# deviation, a test not done left out.
limit <- function(values, sign) {
  mean(values, na.rm = TRUE) + sign * qnorm(0.975) * sd(values, na.rm = TRUE)
}

adlb <- data.frame(
  STUDYID = lb$STUDYID,
  USUBJID = lb$USUBJID,
  PARAMCD = lb$LBTESTCD,
  AVAL = lb$LBSTRESN,
  ANRLO = ave(lb$LBSTRESN, lb$LBTESTCD, FUN = function(x) limit(x, -1)),
  ANRHI = ave(lb$LBSTRESN, lb$LBTESTCD, FUN = function(x) limit(x, 1))
)

dir.create("/app/output", showWarnings = FALSE)
write.csv(adlb, "/app/output/adlb.csv", row.names = FALSE, na = "")
