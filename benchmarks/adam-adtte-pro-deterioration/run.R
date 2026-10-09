library(yamaa)

qs_flagged <- yamaa_domain("spec_qs.yaml")$output
adtte <- yamaa_domain("spec_adtte.yaml")$output
list(qs_flagged = qs_flagged, adtte = adtte)
