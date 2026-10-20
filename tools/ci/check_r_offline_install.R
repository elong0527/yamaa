# Check an R package that was installed without network: it loads from the
# given library and reproduces the expected bytes of the given benchmark, run
# from a temporary copy so the benchmark stays unchanged. The installed package
# carries no test inputs, so the benchmark comes from the checkout.
#
#   Rscript --vanilla tools/ci/check_r_offline_install.R <library> benchmarks/adam-adlb-ordered-sum
arguments <- commandArgs(trailingOnly = TRUE)
library <- arguments[[1L]]
case <- normalizePath(arguments[[2L]], winslash = "/", mustWork = TRUE)
library(yamaa, lib.loc = library)
expected <- list.files(file.path(case, "expected"))
stopifnot(length(expected) > 0L)
work <- tempfile("yamaa-offline-")
dir.create(work)
stopifnot(all(file.copy(file.path(case, c("spec.yaml", "input")), work, recursive = TRUE)))
run <- yamaa_spec(file.path(work, "spec.yaml"))$run()
stopifnot(nrow(run$issues) == 0L)
run$save()
bytes <- function(path) readBin(path, "raw", n = file.size(path))
for (name in expected) {
  stopifnot(identical(bytes(file.path(work, name)), bytes(file.path(case, "expected", name))))
}
unlink(work, recursive = TRUE)
cat("yamaa", format(utils::packageVersion("yamaa", lib.loc = library)),
  "installed offline and reproduced", paste0(basename(case), "\n"))
