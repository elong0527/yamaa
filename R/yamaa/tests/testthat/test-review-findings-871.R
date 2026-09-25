# Regression tests for the PR #871 round-2 review findings:
# eval_case_branch retry must subset per-row identity (driver_ds/driver_rec/
# group_rec/row_keys) alongside col, and strip_regex_classes must scan
# escape-aware so escaped brackets inside classes do not corrupt the
# REQ-0828 capturing-group count.

test_that("case branch retry resolves per-row identity from the taken rows", {
  # S4's partial date makes the eager evaluation fail; the masked retry on
  # the male rows must read S2/S3's own records, not S1/S2's (the pre-fix
  # bug: driver_rec stayed full-length, so sub-frame row 2 resolved S2's
  # record instead of S3's -- silently or via conversion_failed).
  d <- tempfile("case-retry-")
  dir.create(d)
  dir.create(file.path(d, "input"))
  writeLines(c("USUBJID,SEX,DMDTC",
    "S1,F,2024-01-15",
    "S2,M,2024-02-20",
    "S3,M,2024-03-25",
    "S4,F,2024-01"),
    file.path(d, "input", "dm.csv"))
  spec <- c(
    'schema_version: "1.0"',
    'domain: ADSL',
    'keys: [USUBJID]',
    'input:',
    '  DM: input/dm.csv',
    'output:',
    '  path: adsl.csv',
    '  columns: [USUBJID, DTHDT]',
    'columns:',
    '  - name: USUBJID',
    '    type: str',
    '    derivation: DM.USUBJID',
    '  - name: SEX',
    '    type: str',
    '    derivation: DM.SEX',
    '  - name: DTHDT',
    '    type: date',
    '    derivation:',
    '      case:',
    '        - when: "SEX = \'M\'"',
    '          then:',
    '            to_date:',
    '              source: DM.DMDTC')
  writeLines(spec, file.path(d, "spec.yaml"))
  yamaa::run_spec(file.path(d, "spec.yaml"), file.path(d, "adsl.csv"))
  out <- utils::read.csv(file.path(d, "adsl.csv"), colClasses = "character")
  # each taken row resolves its OWN record's date; untaken rows stay missing
  expect_equal(out$USUBJID, c("S1", "S2", "S3", "S4"))
  expect_equal(out$DTHDT, c("", "2024-02-20", "2024-03-25", ""))
})

test_that("strip_regex_classes is escape-aware", {
  # ']' right after '[' is a POSIX literal; an escaped bracket never
  # opens or closes a class
  expect_equal(yamaa:::strip_regex_classes("([\\]])(.)"), "()(.)")
  expect_equal(yamaa:::strip_regex_classes("([\\\\])(.)"), "()(.)")
  expect_equal(yamaa:::strip_regex_classes("(a[b(c]d)"), "(ad)")
  expect_equal(yamaa:::strip_regex_classes("[]](x)"), "(x)")
})

test_that("REQ-0828: group counting survives escaped brackets in classes", {
  # pre-fix, the caller stripped escapes before scanning, so 'a[\\]]b(.)'
  # collapsed to 'a[]b(.)', the class never terminated, the group paren
  # was dropped, and group 1 spuriously raised regex_group_out_of_range
  src <- yamaa:::tv("a]bXc", "str")
  out <- yamaa:::op_str_extract(src, "a[\\]]b(.)", 1)
  expect_equal(out$v, "X")
  # parens inside classes are literals, not groups
  out1 <- yamaa:::op_str_extract(yamaa:::tv("abd", "str"), "(a[b(c]d)", 1)
  expect_equal(out1$v, "abd")
  expect_error(
    yamaa:::op_str_extract(yamaa:::tv("abd", "str"), "(a[b(c]d)", 2),
    "\\[regex_group_out_of_range\\]")
  # genuine out-of-range still raises
  expect_error(
    yamaa:::op_str_extract(yamaa:::tv("ab", "str"), "(a)(b)", 3),
    "\\[regex_group_out_of_range\\]")
})
