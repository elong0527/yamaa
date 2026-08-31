test_that("the shared portable registry is valid", {
  root <- normalizePath(file.path(testthat::test_path(), "../../../.."))
  registry_path <- file.path(root, "yaml/registry/portable-functions.yaml")
  registry <- .load_portable_registry(registry_path)

  expect_identical(registry$core$registry_version, "1.0.0")
  expect_identical(registry$core$namespace, "core")
})

test_that("R passes the shared conformance fixtures", {
  root <- normalizePath(file.path(testthat::test_path(), "../../../.."))
  counts <- .run_portable_conformance(
    file.path(root, "yaml/registry/portable-functions.yaml"),
    file.path(root, "yaml/registry/conformance.yaml")
  )

  expect_gte(unname(counts[["evaluation_cases"]]), 19)
  expect_gte(unname(counts[["validation_cases"]]), 7)
})
