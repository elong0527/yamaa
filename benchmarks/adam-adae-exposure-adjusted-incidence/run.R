library(yamaa)

adrisk <- yamaa_domain("spec_risk.yaml")$output
adinc <- yamaa_domain("spec_rate.yaml")$output
list(adrisk, adinc)
