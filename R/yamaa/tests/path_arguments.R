# R path arguments (#4091): the path of yamaa_spec(), yamaa_project() and
# yamaa_read_odm(), the parent of spec$check() and each path of yamaa_lint()
# take a named or classed character string as its text, and expand a leading
# `~` as base R's file functions do, while a refused path names its argument.
# The seam is R's own argument conversion in api.R: Python's os.fspath()
# expands nothing, so no shared host-fixtures reader reaches it. The classed
# strings carry the classes that fs::path() and glue::glue() give, built with
# structure(), since neither package is a dependency. A `~` path names a file
# beneath the home directory, so the study is written in a scratch directory
# there, removed when the suite ends.
library(yamaa)
source(file.path(normalizePath("host-fixtures", winslash = "/", mustWork = TRUE), "assert.R"),
  local = TRUE)
home <- path.expand("~")
if (identical(home, "~") || !dir.exists(home) || file.access(home, 2L) != 0L) {
  message("Skipped path_arguments.R: a `~` path needs a writable home directory, and `~` names none")
  quit(save = "no")
}

# A study of one specification, its project, a fragment that the
# specification can parent and an ODM document, each valid and each yamaa
# file in the style guide's canonical form as written.
files <- list(
  "input.csv" = "USUBJID,AGE\n01,35\n",
  "dm.yaml" = 'yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [USUBJID]
input: {RAW: {path: input.csv, types: {USUBJID: str, AGE: int}}}

output: {path: dm.csv, columns: [USUBJID, AGE]}

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}
  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}
',
  "age.yaml" = 'yamaa: "1.0"
type: spec

columns:
  - {name: AGE, type: int, label: Age in years, derivation: RAW.AGE}
',
  "project.yaml" = 'yamaa: "1.0"
type: project

sdtm: {specs: [dm.yaml]}
',
  "odm.xml" = '<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3" FileOID="F.1">
  <ClinicalData StudyOID="S.1" MetaDataVersionOID="M.1">
    <SubjectData SubjectKey="01">
      <StudyEventData StudyEventOID="SE.SCREEN">
        <FormData FormOID="F.DM">
          <ItemGroupData ItemGroupOID="IG.DM">
            <ItemData ItemOID="IT.DM.AGE" Value="35"/>
            <ItemData ItemOID="IT.DM.SEX" Value="F"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
')

check_path_arguments <- function() {
  name <- basename(tempfile("yamaa-path-arguments-"))
  scratch <- file.path(home, name)
  stopifnot("the scratch directory is made in the home directory" = dir.create(scratch))
  on.exit(unlink(scratch, recursive = TRUE))
  for (file in names(files)) writeBin(charToRaw(files[[file]]), file.path(scratch, file))
  # The ways to write the paths of `named`, each naming the same files of the
  # scratch directory. Each holds the text `file.path(scratch, named)`, the
  # `~` path once expanded, since the scratch directory is `~/<name>`.
  forms <- function(named) {
    paths <- file.path(scratch, named)
    list("a named string" = setNames(paths, named),
      "an fs_path string" = structure(paths, class = c("fs_path", "character")),
      "a glue string" = structure(paths, class = c("glue", "character")),
      "a `~` path" = file.path("~", name, named))
  }
  fragment <- yamaa_spec(file.path(scratch, "age.yaml"))
  # Each path argument: the files it names, its call, and what the call
  # gives once it reads those files: a specification and a project print
  # their path (D37), a valid file checks as succeeded, a project lists its
  # domain, the ODM document's items are its subject's two, and the study,
  # canonical as written, has no lint finding.
  cases <- list(
    list(label = "yamaa_spec(path)", files = "dm.yaml", call = function(path) {
      spec <- yamaa_spec(path)
      list(format = format(spec), status = spec$check()$status)
    }, expected = list(format = paste("yamaa spec", file.path(scratch, "dm.yaml")),
      status = "succeeded")),
    list(label = "spec$check(parent)", files = "dm.yaml",
      call = function(path) list(status = fragment$check(parent = path)$status),
      expected = list(status = "succeeded")),
    list(label = "yamaa_project(path)", files = "project.yaml", call = function(path) {
      project <- yamaa_project(path)
      list(format = format(project), names = project$names)
    }, expected = list(format = paste("yamaa project", file.path(scratch, "project.yaml")),
      names = "DM")),
    list(label = "yamaa_read_odm(source)", files = "odm.xml", call = function(path) {
      as.list(yamaa_read_odm(path)[c("SubjectKey", "ItemOID", "Value")])
    }, expected = list(SubjectKey = c("01", "01"), ItemOID = c("IT.DM.AGE", "IT.DM.SEX"),
      Value = c("35", "F"))),
    list(label = "yamaa_lint(paths)", files = c("dm.yaml", "age.yaml", "project.yaml"),
      call = function(paths) {
        findings <- yamaa_lint(paths)
        list(columns = names(findings), rows = nrow(findings))
      }, expected = list(columns = c("file", "line", "column", "finding", "spec_path", "message"),
        rows = 0L)))
  for (case in cases) {
    values <- forms(case$files)
    for (form in names(values)) {
      outcome <- tryCatch(case$call(values[[form]]),
        error = function(error) list(error = conditionMessage(error)))
      expect_identical(outcome, case$expected,
        sprintf("%s reads the files that %s names", case$label, form))
    }
  }
  cat("a named, fs_path or glue string and a `~` path name their file in every path argument\n")

  # A value that is no path is refused with an ordinary error naming its
  # argument, in the words of api.R's conversions: one path takes one
  # non-missing string, and yamaa_lint() a non-empty vector of them.
  one <- function(what) sprintf("`%s` must be one non-missing character string", what)
  single <- list("a number" = 1, "a missing string" = NA_character_,
    "two strings" = c("dm.yaml", "age.yaml"), "no string" = character(), "a list" = list("dm.yaml"))
  refusals <- list(
    list(label = "yamaa_spec(path)", call = yamaa_spec, message = one("path"), values = single),
    list(label = "spec$check(parent)", call = function(value) fragment$check(parent = value),
      message = one("parent"), values = single),
    list(label = "yamaa_project(path)", call = yamaa_project, message = one("path"),
      values = single),
    list(label = "yamaa_read_odm(source)", call = yamaa_read_odm, message = one("source"),
      values = single),
    list(label = "yamaa_lint(paths)", call = yamaa_lint,
      message = "`paths` must be a non-empty character vector without missing values",
      values = list("a number" = 1, "a missing path" = c("dm.yaml", NA_character_),
        "no path" = character(), "a list" = list("dm.yaml"))))
  for (refusal in refusals) {
    for (value in names(refusal$values)) {
      outcome <- tryCatch({
        refusal$call(refusal$values[[value]])
        list(class = "returned", message = NA_character_, call = NULL)
      }, error = function(error) {
        list(class = class(error), message = conditionMessage(error), call = conditionCall(error))
      })
      expect_identical(outcome,
        list(class = c("simpleError", "error", "condition"), message = refusal$message, call = NULL),
        sprintf("%s refuses %s naming its argument", refusal$label, value))
    }
  }
  cat("a refused path names its argument\n")
}
check_path_arguments()
