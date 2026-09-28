# REQ-1263: an intermediate read is matched to each donor record before a
# later derivation windows over the augmented donor frame.

issue_1393_fixture <- function() {
  root <- tempfile("intermediate-read-")
  dir.create(root)
  dir.create(file.path(root, "input"))
  writeLines(c("USUBJID,LBSEQ,LBTESTCD", "A,1,HGB", "A,2,HGB", "B,1,HGB"),
    file.path(root, "input", "lb.csv"))
  writeLines(c("USUBJID,IDVARVAL,QNAM,QVAL", "A,  1,ENDPOINT,Y",
    "A,  2,OTHER,N", "B,  9,ENDPOINT,Y"),
    file.path(root, "input", "supp.csv"))
  spec <- c(
    'schema_version: "1.0"',
    'domain: ADLB',
    'keys: [USUBJID, LBSEQ]',
    'input:',
    '  LB: {path: input/lb.csv, types: {LBSEQ: int}}',
    '  SUPP: input/supp.csv',
    'base: LB',
    'intermediates:',
    '  - id: SUPP_EP',
    '    dataset: SUPP',
    '    filter: "SUPP.QNAM = \'ENDPOINT\'"',
    '    key:',
    '      USUBJID: LB.USUBJID',
    '      IDVARVAL: {str_pad: {source: LB.LBSEQ, width: 3}}',
    '    columns: [QVAL]',
    '    no_match: null',
    '  - id: RANK',
    '    dataset: LB',
    '    key: [USUBJID, LBSEQ]',
    '    derivations:',
    '      ENDPOINT: SUPP_EP.QVAL',
    '      EOT_SEQ:',
    '        row_number:',
    '          window:',
    '            group_by: [LB.USUBJID, LB.LBTESTCD]',
    '            order_by:',
    '              - {variable: ENDPOINT, direction: desc}',
    '              - {variable: LB.LBSEQ, direction: desc}',
    'output:',
    '  path: adlb.csv',
    '  columns: [USUBJID, LBSEQ, ENDPOINT, EOT_SEQ]',
    'columns:',
    '  - {name: USUBJID, type: str, derivation: LB.USUBJID}',
    '  - {name: LBSEQ, type: int, derivation: LB.LBSEQ}',
    '  - {name: ENDPOINT, type: str, derivation: RANK.ENDPOINT}',
    '  - {name: EOT_SEQ, type: int, derivation: RANK.EOT_SEQ}')
  list(root = root, spec = spec)
}

issue_1393_run <- function(fixture, spec = fixture$spec) {
  path <- file.path(fixture$root, "spec.yaml")
  writeLines(spec, path)
  out <- file.path(fixture$root, "adlb.csv")
  run_spec(path, out)
  utils::read.csv(out, colClasses = "character", na.strings = character(0),
    check.names = FALSE)
}

issue_1393_condition <- function(fixture, spec) {
  tryCatch({
    issue_1393_run(fixture, spec)
    NA_character_
  }, yamaa_error = function(e) attr(e, "yamaa_condition"))
}

test_that("an intermediate read is matched per donor before its window", {
  fixture <- issue_1393_fixture()
  out <- issue_1393_run(fixture)
  stopifnot(identical(out$USUBJID, c("A", "A", "B")))
  stopifnot(identical(out$ENDPOINT, c("Y", "", "")))
  stopifnot(identical(out$EOT_SEQ, c("1", "2", "1")))
})

test_that("a donor read follows the referenced intermediate's absence policy", {
  fixture <- issue_1393_fixture()
  spec <- fixture$spec[fixture$spec != "    no_match: null"]
  stopifnot(identical(issue_1393_condition(fixture, spec), "unmatched_key"))
})

test_that("a donor read respects the referenced intermediate's columns", {
  fixture <- issue_1393_fixture()
  spec <- sub("columns: \\[QVAL\\]", "columns: [IDVARVAL]", fixture$spec)
  stopifnot(identical(issue_1393_condition(fixture, spec), "unknown_field"))
})

test_that("a mapped match can read an earlier donor derivation", {
  fixture <- issue_1393_fixture()
  spec <- sub("IDVARVAL: \\{str_pad: \\{source: LB.LBSEQ, width: 3\\}\\}",
    "IDVARVAL: LB.JOIN_ID", fixture$spec)
  spec <- append(spec,
    "      JOIN_ID: {str_pad: {source: LB.LBSEQ, width: 3}}",
    after = which(spec == "    derivations:"))
  out <- issue_1393_run(fixture, spec)
  stopifnot(identical(out$ENDPOINT, c("Y", "", "")))
})

test_that("another intermediate on the same dataset starts with stored fields", {
  fixture <- issue_1393_fixture()
  spec <- append(fixture$spec,
    c("  - id: COPY", "    dataset: LB", "    key: [USUBJID, LBSEQ]",
      "    derivations:", "      ENDPOINT: LB.LBSEQ"),
    after = which(fixture$spec == "    no_match: null"))
  spec <- append(spec, "      SEQ_COPY: COPY.ENDPOINT",
    after = which(spec == "      ENDPOINT: SUPP_EP.QVAL"))
  out <- issue_1393_run(fixture, spec)
  stopifnot(identical(out$EOT_SEQ, c("1", "2", "1")))
})

test_that("a window cannot read another intermediate directly", {
  fixture <- issue_1393_fixture()
  spec <- sub("variable: ENDPOINT", "variable: SUPP_EP.QVAL", fixture$spec)
  stopifnot(identical(issue_1393_condition(fixture, spec), "unknown_field"))
})

test_that("a donor derivation cannot bypass an intermediate with a raw dataset", {
  fixture <- issue_1393_fixture()
  spec <- sub("ENDPOINT: SUPP_EP.QVAL", "ENDPOINT: SUPP.QVAL", fixture$spec)
  stopifnot(identical(issue_1393_condition(fixture, spec), "unknown_field"))
})

test_that("a donor derivation cannot read a SELF intermediate", {
  fixture <- issue_1393_fixture()
  spec <- append(fixture$spec,
    c("  - id: SELF_READ", "    dataset: SELF", "    key: [USUBJID, LBSEQ]",
      "    no_match: null"),
    after = which(fixture$spec == "    no_match: null"))
  spec <- sub("ENDPOINT: SUPP_EP.QVAL", "ENDPOINT: SELF_READ.QVAL", spec)
  stopifnot(identical(issue_1393_condition(fixture, spec), "phase_boundary"))
})

test_that("intermediates cannot read each other in a cycle", {
  fixture <- issue_1393_fixture()
  spec <- append(fixture$spec,
    c("    derivations:", "      QVAL_COPY: RANK.ENDPOINT"),
    after = which(fixture$spec == "    no_match: null") - 1L)
  spec <- sub("ENDPOINT: SUPP_EP.QVAL", "ENDPOINT: SUPP_EP.QVAL_COPY", spec)
  stopifnot(identical(issue_1393_condition(fixture, spec), "dependency_cycle"))
})
