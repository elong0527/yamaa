# Check an R package that was installed without network: it loads from the
# given library and reproduces the expected bytes of one case it installs, run
# from a temporary copy so the installed tree stays unchanged.
#
#   Rscript --vanilla tools/ci/check_r_offline_install.R <library>
library <- commandArgs(trailingOnly = TRUE)[[1L]]
library(yamaa, lib.loc = library)
case <- system.file("specification-original", "cases", "adam-adlb-ordered-sum",
  package = "yamaa", lib.loc = library, mustWork = TRUE)
work <- tempfile("yamaa-offline-")
dir.create(work)
stopifnot(all(file.copy(file.path(case, c("spec.yaml", "input")), work, recursive = TRUE)))
run <- yamaa_spec(file.path(work, "spec.yaml"))$run()
stopifnot(nrow(run$issues) == 0L)
run$save()
bytes <- function(path) readBin(path, "raw", n = file.size(path))
stopifnot(identical(bytes(file.path(work, "adlb.csv")), bytes(file.path(case, "expected", "adlb.csv"))))
unlink(work, recursive = TRUE)
cat("yamaa", format(utils::packageVersion("yamaa", lib.loc = library)),
  "installed offline and reproduced adam-adlb-ordered-sum\n")
