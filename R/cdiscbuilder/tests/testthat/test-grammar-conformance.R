test_that("the R parsers reproduce every shared grammar vector", {
  root <- normalizePath(
    file.path("..", "..", "..", ".."),
    mustWork = FALSE
  )
  skip_if_not(
    dir.exists(file.path(root, "yaml", "grammar")),
    "the shared grammar files are not beside the package"
  )
  runner <- file.path(
    root, "R", "cdiscbuilder", "inst", "conformance",
    "grammar_conformance.R"
  )
  skip_if_not(file.exists(runner), "the conformance runner is not beside it")
  source(runner, local = TRUE)
  expect_equal(yamaa_grammar_conformance(root), character(0))
})

test_that("a shape distinguishes parses an outcome alone cannot", {
  expect_equal(
    yamaa_grammar_decision("predicate", "A = 1 OR B = 2 AND C = 3")$shape,
    "(or (= (id A) (int 1)) (and (= (id B) (int 2)) (= (id C) (int 3))))"
  )
  expect_equal(
    yamaa_grammar_decision("numeric", "-A * B")$shape,
    "(* (neg (id A)) (id B))"
  )
})

test_that("a closed vocabulary rejects a name outside it", {
  expect_equal(
    yamaa_grammar_decision("numeric", "ROUND(A, 2)")$condition,
    "prohibited_function"
  )
  expect_equal(
    yamaa_grammar_decision("aggregate", "MAX(SUM(A))")$condition,
    "nested_reduction"
  )
  expect_equal(
    yamaa_grammar_decision("predicate", "AGE != 18")$condition,
    "invalid_predicate"
  )
  expect_equal(
    yamaa_grammar_decision("string-template", "{A + B}")$condition,
    "invalid_string_template"
  )
})
