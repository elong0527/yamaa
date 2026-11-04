# Installed yamaa_cli(): the command line run as the R host (cli.entries of
# rust/interface/bindings.yaml). The commands, what they print and their exit
# statuses are the facade's cli module, which the yamaa binary's own tests
# cover line by line; this checks the seam the binary cannot reach: the
# command runs over this R's engine, so it names this host, writes to R's
# standard output and standard error, and calls the project functions
# installed here, which the binary refuses. Python reads the same seam.
library(yamaa)
# The truth is JSON, and jsonlite, which reads it, is a suggested package:
# where it is not installed, the file is skipped.
if (!requireNamespace("jsonlite", quietly = TRUE)) {
  message("Skipped cli.R: its output is JSON, and jsonlite, a suggested package, is not installed")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)

# yamaa_cli(arguments): its exit status, and what it wrote to standard output,
# read back as text. Standard error is R's own stream, which the engine writes
# through REprintf, so a sink does not divert it.
run <- function(arguments) {
  out <- tempfile("yamaa-cli-out-")
  on.exit(unlink(out))
  sink(out)
  status <- tryCatch(yamaa_cli(arguments), finally = sink())
  list(status = status, stdout = paste(readLines(out, warn = FALSE), collapse = "\n"))
}

stopifnot("yamaa_cli() takes the arguments" = identical(names(formals(yamaa_cli)), "arguments"),
  "yamaa_cli() is exported" = "yamaa_cli" %in% getNamespaceExports("yamaa"))
cat("yamaa_cli() takes the arguments and is exported\n")

info <- yamaa_info()
version <- run(c("--format", "json", "--version"))
expect_identical(version$status, 0L, "--version exits 0")
printed <- jsonlite::fromJSON(version$stdout, simplifyVector = FALSE)
expect_identical(printed[c("version", "host", "host_version", "platform")],
  list(version = info$version, host = "r", host_version = as.character(getRversion()), platform = info$platform),
  "--version names this host and this R")
expect_identical(unlist(printed$features), info$features, "--version lists the engine's features")
text <- run("--version")
expect_identical(text$stdout, paste0("yamaa ", info$version, "\nlanguage versions: ",
  paste(info$language_versions, collapse = ", "), "\nfeatures: ", paste(info$features, collapse = ", "),
  "\nhost: r\nhost version: ", info$host_version, "\nplatform: ", info$platform),
  "--version prints the host version on its own line")
cat("--version names this host and this R\n")

invisible_status <- withVisible(yamaa_cli(c("schema", "spec")))
stopifnot("the status is returned invisibly" = !invisible_status$visible,
  "the status is an integer" = identical(invisible_status$value, 0L))
cat("yamaa_cli() returns the exit status invisibly\n")

refused <- run("bogus")
expect_identical(refused$status, 2L, "an unknown command exits 2")
expect_identical(refused$stdout, "", "a usage error prints nothing on standard output")
cat("a usage error exits 2\n")

for (arguments in list(NA_character_, 1, list("run"), NULL)) {
  error <- tryCatch(yamaa_cli(arguments), error = identity)
  stopifnot_case(paste("the argument", deparse(arguments)), "the mistake is a package error" =
    inherits(error, "error") && inherits(error, "yamaa_error"))
}
cat("arguments must be a character vector without missing values\n")

# The binary refuses this run before it reads data, as unsupported_operation
# (D110, D123): the R host runs it through base::signif, the installed
# implementation the base-round study names, verifies the project's renv lock
# first, and saves the authored bytes.
root <- tempfile("yamaa-cli-")
dir.create(root)
stopifnot("the authored study is copied" = file.copy("project-base/base-round", root, recursive = TRUE))
study <- normalizePath(file.path(root, "base-round"), winslash = "/", mustWork = TRUE)
output <- file.path(study, "test.csv")
bytes <- function(path) readBin(path, "raw", n = file.info(path)$size)
ran <- run(c("--format", "json", "run", file.path(study, "project.yaml"), "--spec", "TEST"))
expect_identical(ran$status, 0L, "the run exits 0")
document <- jsonlite::fromJSON(ran$stdout, simplifyVector = FALSE)
expect_identical(document[c("status", "spec", "issues", "paths")],
  list(status = "succeeded", spec = "TEST", issues = list(), paths = list(output)),
  "the run succeeds and writes the dataset")
expect_identical(bytes(output), bytes(file.path(study, "expected/test.csv")),
  "the R function's output matches the authored CSV")
unlink(output)
whole <- run(c("run", file.path(study, "project.yaml")))
expect_identical(whole$status, 0L, "the project run exits 0")
expect_identical(whole$stdout, paste0("yamaa project run of ", file.path(study, "project.yaml"),
  ": succeeded, 1 specification, 0 issues\n  TEST  succeeded  3 rows x 2 columns, 0 issues\n", output),
  "the project run prints its summary, its member and the path written")
expect_identical(bytes(output), bytes(file.path(study, "expected/test.csv")),
  "the project run saves the authored bytes")
unlink(root, recursive = TRUE)
cat("a run calls the project functions installed here\n")
