test_that("YAML 1.2 core scalars keep their specified types", {
  path <- tempfile(fileext = ".yaml")
  on.exit(unlink(path))
  writeLines(c(
    "Y: y", "y: Y", "N: n", "n: N",
    "yes: no", "on: off", "date: 2026-09-29",
    "true_value: true", "false_value: FALSE", "null_value: null"
  ), path)
  doc <- yamaa:::yaml_load_file(path)

  expect_identical(names(doc)[1:4], c("Y", "y", "N", "n"))
  expect_identical(unname(unlist(doc[1:4])), c("y", "Y", "n", "N"))
  expect_identical(doc$yes, "no")
  expect_identical(doc$on, "off")
  expect_identical(doc$date, "2026-09-29")
  expect_identical(doc$true_value, TRUE)
  expect_identical(doc$false_value, FALSE)
  expect_true("null_value" %in% names(doc))
  expect_null(doc$null_value)
})

test_that("YAML numbers retain exact int32 minimum and normalize non-finite values", {
  path <- tempfile(fileext = ".yaml")
  on.exit(unlink(path))
  writeLines(c(
    "minimum: -2147483648", "maximum_plus_one: 2147483648",
    "mixed: [-2147483648, null, 2]",
    "mixed_float: [-2147483648, 2.5]",
    "infinity: .inf", "negative_infinity: -.inf", "nan: .nan"
  ), path)
  doc <- yamaa:::yaml_load_file(path)

  expect_identical(doc$minimum, -2147483648)
  expect_identical(doc$maximum_plus_one, 2147483648)
  expect_identical(doc$mixed, c(-2147483648, NA_real_, 2))
  expect_identical(doc$mixed_float, c(-2147483648, 2.5))
  expect_true(all(is.na(c(doc$infinity, doc$negative_infinity, doc$nan))))
})

test_that("boolean literals are not silently recast as Y and N strings", {
  expect_identical(yamaa:::literal_to_tv(TRUE, NULL, 1)$t, "bool")
  expect_identical(yamaa:::literal_to_tv("Y", NULL, 1)$v, "Y")
  expect_error(yamaa:::literal_to_tv(TRUE, "str", 1), "conversion_failed")
})

test_that("mapping keeps Y distinct from the string TRUE", {
  ctx <- list(n = 2, col = list(SRC = yamaa:::tv(c("Y", "TRUE"), "str")))
  dict <- list(Y = "alpha", `TRUE` = "beta")
  out <- yamaa:::eval_mapping(list(source = "SRC", dict = dict), ctx)
  expect_identical(out$v, c("alpha", "beta"))
})
