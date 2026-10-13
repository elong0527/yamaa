# Fresh whole-run replay of the retained qualified source archive, outside checkout.
args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 2L)
library_path <- normalizePath(args[[1L]], mustWork = TRUE)
proof <- normalizePath(args[[2L]], mustWork = TRUE)

# Verify the prior package before creating study inputs or constructing any run.
library(yamaa, lib.loc = library_path)
stopifnot(identical(as.character(utils::packageVersion("yamaa", lib.loc = library_path)), "0.3.0"),
  identical(normalizePath(system.file(package = "yamaa", mustWork = TRUE)),
    normalizePath(file.path(library_path, "yamaa"))))
writeLines(c("installed_package_version=0.3.0", paste0("installed_package=", system.file(package = "yamaa")),
  paste0("R_version=", R.version.string), paste0("R_platform=", R.version$platform),
  "version_checked_before_study_data=true"), file.path(proof, "installed-version.txt"))
capture.output(sessionInfo(), file = file.path(proof, "session-info.txt"))

root <- file.path(proof, "replay")
stopifnot(dir.create(root))
path <- function(name) file.path(root, name)
rawfile <- function(name) readBin(path(name), "raw", n = file.info(path(name))$size)
write <- function(name, text) writeBin(charToRaw(text), path(name))
prior_saved <- charToRaw("saved candidate output remains owned by its original run\n")
writeBin(prior_saved, path("candidate-output.csv"))
spec <- paste0("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n",
  "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n",
  "  - {name: VALUE, type: int, derivation: SRC.VALUE}\n",
  "output: {path: rollback-output.csv, columns: [ID, VALUE]}\n")
input <- "ID,VALUE\n001,9223372036854775807\n007,9007199254740993\n"
write("spec.yaml", spec)
write("input.csv", input)
checked <- yamaa_check(path("spec.yaml"))
stopifnot(is.data.frame(checked$issues), nrow(checked$issues) == 0L)
replayed <- yamaa_domain(path("spec.yaml"))
stopifnot(nrow(replayed$issues) == 0L,
  identical(replayed$output$ID, c("001", "007")),
  identical(as.character(replayed$output$VALUE), c("9223372036854775807", "9007199254740993")),
  !file.exists(path("rollback-output.csv")), identical(rawfile("candidate-output.csv"), prior_saved))
unlink(path(c("spec.yaml", "input.csv")))
gc()
for (attempt in 1:2) stopifnot(identical(replayed$save(), TRUE),
  identical(rawfile("rollback-output.csv"), charToRaw(input)),
  identical(rawfile("candidate-output.csv"), prior_saved))

# Define replay uses the archive's independently authored, previously qualified goldens.
fixtures <- system.file("define-fixtures", "sdtm-dm-metadata", package = "yamaa", mustWork = TRUE)
defined_root <- path("define")
stopifnot(dir.create(defined_root),
  all(file.copy(file.path(fixtures, c("spec.yaml", "environment.yaml")), defined_root)),
  file.copy(file.path(fixtures, "expected", "dm.csv"), defined_root))
defined <- yamaa_define(file.path(defined_root, "environment.yaml"), "sdtm", created = "2026-09-11T00:00:00")
stopifnot(nrow(defined$issues) == 0L)
unlink(file.path(defined_root, c("spec.yaml", "environment.yaml", "dm.csv")))
gc()
stopifnot(isTRUE(defined$save()))
for (name in c("dm.json", "define.xml")) {
  actual <- file.path(defined_root, name)
  expected <- file.path(fixtures, "expected", name)
  stopifnot(identical(readBin(actual, "raw", n = file.info(actual)$size),
    readBin(expected, "raw", n = file.info(expected)$size)))
}
stopifnot(identical(rawfile("candidate-output.csv"), prior_saved))
writeLines(c("rehearsal=pass", "fresh_isolated_library=true", "whole_run_prior_release=true",
  "domain_check_define=true", "full_i64_and_leading_zero_text=true", "held_save_after_input_removal=true",
  "saved_candidate_output_preserved=true", "published_previous_R_release=false",
  "durable_release_asset_retention=pending"), file.path(proof, "rehearsal-result.txt"))
cat("fresh-library R source rollback replay passed; saved candidate output remains unchanged\n")
