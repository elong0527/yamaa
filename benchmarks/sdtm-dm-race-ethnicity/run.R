library(yamaa)

dm <- yamaa_domain("spec_dm.yaml")$output
suppdm <- yamaa_domain("spec_suppdm.yaml")$output
list(dm = dm, suppdm = suppdm)
