submission_root <- normalizePath(testthat::test_path("..", "..", "..", ".."))
submission_schema_root <- file.path(submission_root, "yaml")
submission_example <- file.path(submission_root, "benchmarks", "sdtm-dm-metadata")

test_that("the canonical submission schemas stay identical in the package", {
  for (name in c("schema_shared.yaml", "schema_metadata.yaml", "schema_define.yaml")) {
    canonical <- readBin(file.path(submission_schema_root, name), "raw", n = 1000000L)
    installed <- readBin(file.path(submission_root, "R", "cdiscbuilder", "inst", "submission", name), "raw", n = 1000000L)
    expect_identical(installed, canonical)
  }
})

test_that("all committed study documents load with no terminology diagnostics", {
  for (name in c("sdtm-dm-metadata", "sdtm-ae-coding", "sdtm-vs-epoch-from-subject-elements")) {
    study <- load_study_document(file.path(submission_root, "benchmarks", name, "define.yaml"), schema_root = submission_schema_root)
    expect_length(validate_study_terminology(study$document, study$specifications), 0L)
    expect_length(validate_submission_metadata(study$specifications[[1L]]), 0L)
  }
})

test_that("submission metadata fails with portable conditions", {
  spec <- yaml12::read_yaml(file.path(submission_example, "spec.yaml"), simplify = FALSE)
  cases <- list(
    list(change = function(x) { x$columns[[1L]]$submission$data_type <- "integer"; x }, condition = "submission_data_type_not_admitted"),
    list(change = function(x) { x$columns[[1L]]$submission$length <- NULL; x }, condition = "submission_length_missing"),
    list(change = function(x) { x$columns[[1L]]$submission$length <- 0; x }, condition = "submission_length_invalid"),
    list(change = function(x) { x$columns[[1L]]$submission$origin <- NULL; x }, condition = "origin_missing"),
    list(change = function(x) { x$columns[[1L]]$submission$origin$type <- "Derived"; x }, condition = "method_missing"),
    list(change = function(x) { x$columns[[1L]]$submission$origin$type <- "Collected"; x }, condition = "origin_contradicts_derivation"),
    list(change = function(x) { x$metadata <- list(label = "reserved"); x }, condition = "reserved_metadata_key")
  )
  for (case in cases) {
    diagnostics <- validate_submission_metadata(case$change(spec))
    expect_true(case$condition %in% vapply(diagnostics, function(x) x$condition, ""))
  }
})

test_that("codelist shape and bindings fail with the Python conditions", {
  study <- load_study_document(file.path(submission_example, "define.yaml"), schema_root = submission_schema_root)
  document <- study$document
  document$codelists[[3L]]$items[[4L]] <- document$codelists[[3L]]$items[[1L]]
  diagnostics <- validate_study_terminology(document, study$specifications)
  expect_true("codelist_duplicate_value" %in% vapply(diagnostics, function(x) x$condition, ""))
  expect_identical(diagnostics[[1L]]$spec_paths, list("codelists.SEX.items[3]"))
  document <- study$document
  document$codelists[[3L]]$items[[2L]]$decode <- NULL
  diagnostics <- validate_study_terminology(document, study$specifications)
  expect_identical(diagnostics[[1L]]$condition, "codelist_partial_item_field")
  document <- study$document
  document$codelists[[3L]]$items <- document$codelists[[3L]]$items[1:2]
  diagnostics <- validate_study_terminology(document, study$specifications)
  expect_identical(diagnostics[[1L]]$condition, "codelist_values_conflict")
  document$codelists[[3L]]$extensible <- TRUE
  expect_length(validate_study_terminology(document, study$specifications), 0L)
})

test_that("shape validation rejects unknown submission declarations", {
  spec <- yaml12::read_yaml(file.path(submission_example, "spec.yaml"), simplify = FALSE)
  spec$columns[[1L]]$submission$typo <- "unknown"
  schema <- .submission_schema(submission_schema_root)
  diagnostics <- .submission_spec_shape(spec, schema)
  expect_identical(diagnostics[[1L]]$condition, "unknown_field")
  expect_identical(diagnostics[[1L]]$spec_paths, list("columns.DOMAIN.submission.typo"))
})

test_that("schema defaults and scalar types retain their declared meaning", {
  schema <- .submission_schema(submission_schema_root)
  normalized <- .submission_normalize(list(type = "Assigned"), "submission_origin_class", schema)
  expect_identical(normalized$type, "Assigned")
  method <- .submission_normalize(list(description = "Algorithm"), "submission_method_class", schema)
  expect_identical(method$type, "Computation")
  expect_length(.submission_shape(1L, "int", schema, "length"), 0L)
  expect_identical(.submission_shape(1.0, "int", schema, "length")[[1L]]$condition, "invalid_field_type")
  expect_length(.submission_shape(Inf, "float", schema, "value"), 1L)
})

test_that("terminology rejects unbound, mistyped, and malformed declarations", {
  study <- load_study_document(file.path(submission_example, "define.yaml"), schema_root = submission_schema_root)
  cases <- list(
    list(change = function(x) { x$codelists[[3L]]$id <- "UNKNOWN"; x }, condition = "unknown_codelist"),
    list(change = function(x) { x$codelists[[3L]]$data_type <- "integer"; x }, condition = "codelist_type_mismatch"),
    list(change = function(x) { x$codelists[[3L]]$standard <- "SDTMIG"; x }, condition = "codelist_shape_invalid"),
    list(change = function(x) { x$codelists[[3L]]$external <- list(dictionary = "D", version = "1"); x }, condition = "codelist_shape_invalid"),
    list(change = function(x) { x$codelists[[3L]]$items[[1L]]$extended <- TRUE; x }, condition = "codelist_extension_not_admitted"),
    list(change = function(x) { x$codelists[[3L]]$items[[1L]]$rank <- 1L; x }, condition = "codelist_partial_item_field"),
    list(change = function(x) { x$codelists[[4L]]$name <- x$codelists[[3L]]$name; x }, condition = "duplicate_define_identifier"),
    list(change = function(x) { x$codelists[[5L]] <- list(id = "UNUSED", name = "Unused", items = list(list(value = "X"))); x }, condition = "unreferenced_codelist")
  )
  for (case in cases) {
    diagnostics <- validate_study_terminology(case$change(study$document), study$specifications)
    expect_true(case$condition %in% vapply(diagnostics, function(x) x$condition, ""))
  }
  document <- list(standards = list(), codelists = list(
    list(id = "N", name = "Number", data_type = "integer", items = list(list(value = 1.0), list(value = NA_integer_)))))
  diagnostics <- validate_study_terminology(document, list())
  expect_identical(vapply(diagnostics, function(x) x$condition, ""),
                   c("codelist_shape_invalid", "codelist_shape_invalid", "unreferenced_codelist"))
})

test_that("row origins override column derivations and inherit their shared method", {
  spec <- yaml12::read_yaml(file.path(submission_root, "benchmarks", "sdtm-lb-metadata", "spec.yaml"), simplify = FALSE)
  expect_length(validate_submission_metadata(spec), 0L)
  name <- "LBORRESU"
  position <- match(name, vapply(spec$columns, function(x) x$name, ""))
  spec$columns[[position]]$derivation <- list(compute = "1 + 1")
  spec$columns[[position]]$submission$origin$type <- "Protocol"
  for (i in seq_along(spec$rows)) spec$rows[[i]]$derivations[[name]] <- list(literal = "mg/dL")
  expect_length(validate_submission_metadata(spec), 0L)
  spec$rows[[1L]]$submission[[name]]$origin$type <- "Derived"
  spec$rows[[1L]]$derivations[[name]] <- list(compute = "1 + 1")
  spec$columns[[position]]$submission$method <- "Shared method"
  diagnostics <- validate_submission_metadata(spec)
  expect_false("method_missing" %in% vapply(diagnostics, function(x) x$condition, ""))
  spec$rows[[1L]]$derivations$LBTESTCD <- list(source = "RAW.CODE")
  diagnostics <- validate_submission_metadata(spec)
  expect_true("value_metadata_requires_literal_testcd" %in% vapply(diagnostics, function(x) x$condition, ""))
})

test_that("declaration comparison ignores mapping key order without hashing", {
  left <- list(origin = list(type = "Assigned", source = "Sponsor"), codelist = "UNIT")
  right <- list(codelist = "UNIT", origin = list(source = "Sponsor", type = "Assigned"))
  expect_true(.submission_equal_declarations(left, right))
  right$codelist <- "OTHER"
  expect_false(.submission_equal_declarations(left, right))
})

test_that("runtime codelists report column, keys, and offending values", {
  study <- load_study_document(file.path(submission_example, "define.yaml"), schema_root = submission_schema_root)
  spec <- study$specifications$DM
  column <- spec$columns[[8L]]
  data <- data.frame(STUDYID = c("S", "S", "S"), USUBJID = c("1", "2", "3"), SEX = c("X", NA, "F"))
  findings <- check_submission_column(data, column, spec, study$document)
  expect_length(findings, 1L)
  expect_identical(findings[[1L]]$requirement, "REQ-0957")
  expect_identical(findings[[1L]]$context$values, list("X"))
  expect_identical(findings[[1L]]$context$keys, list(list(STUDYID = "S", USUBJID = "1")))
  study$document$codelists[[3L]]$extensible <- TRUE
  expect_length(check_submission_column(data, column, spec, study$document), 0L)
})

test_that("row-level codelists replace the shared binding for each test code", {
  specification <- list(domain = "LB", keys = list("ID"), rows = list(
    list(id = "glucose", derivations = list(LBTESTCD = list(literal = "GLUC")),
         submission = list(LBORRESU = list(codelist = "GLUCUNIT")))))
  column <- list(name = "LBORRESU", submission = list(codelist = "UNIT"))
  document <- list(codelists = list(
    list(id = "UNIT", items = list(list(value = "g/L"))),
    list(id = "GLUCUNIT", items = list(list(value = "mg/dL")))))
  data <- data.frame(ID = c("1", "2"), LBTESTCD = c("GLUC", "OTHER"), LBORRESU = c("mg/dL", "g/L"))
  expect_length(check_submission_column(data, column, specification, document), 0L)
  data$LBORRESU[1L] <- "g/L"
  findings <- check_submission_column(data, column, specification, document)
  expect_identical(findings[[1L]]$context$codelist, "GLUCUNIT")
})
