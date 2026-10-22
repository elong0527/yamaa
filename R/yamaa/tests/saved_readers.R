# Independent reader controls run on the installed package, never a source import.
library(yamaa)
# The readers are arrow and jsonlite at the versions that DESCRIPTION suggests,
# and the truth is JSON. A suggested package may not be installed, so without
# them the file is skipped.
if (!requireNamespace("arrow", quietly = TRUE) || utils::packageVersion("arrow") < "23.0.1" ||
    !requireNamespace("jsonlite", quietly = TRUE) || utils::packageVersion("jsonlite") < "2.0.0") {
  message("Skipped saved_readers.R: it needs the suggested packages arrow (>= 23.0.1) and jsonlite (>= 2.0.0)")
  quit(save = "no")
}
host_fixture_root <- normalizePath("host-fixtures", winslash = "/", mustWork = TRUE)
source(file.path(host_fixture_root, "assert.R"), local = TRUE)
truth <- jsonlite::fromJSON(file.path(host_fixture_root, "saved-readers.json"), simplifyVector = FALSE)
expect_identical(truth$version, "1", "the saved-readers truth's version")
columns <- unlist(truth$columns, use.names = FALSE)
kinds <- unlist(truth$types, use.names = FALSE)
# A source the case names: nonempty, or its header alone.
source_label <- function(empty) if (empty) "a header-only source" else "a nonempty source"
strings <- function(values) vapply(values, function(value) if (is.null(value)) NA_character_ else
  as.character(value), character(1))
float_hex <- function(values) vapply(values, function(value) {
  if (is.na(value)) return(NA_character_)
  paste(format(writeBin(as.double(value), raw(), size = 8L, endian = "big")), collapse = "")
}, character(1))
arrow_types <- list(str = arrow::utf8(), int = arrow::int64(), float = arrow::float64(),
                    date = arrow::date32(), datetime = arrow::timestamp("us"))

check_parquet <- function(saved, empty) {
  table <- arrow::read_parquet(arrow::BufferReader$create(saved), as_data_frame = FALSE)
  label <- paste("the Parquet output of", source_label(empty))
  expect_identical(names(table), columns, paste(label, "has the authored columns"))
  stopifnot_case(label, "it holds every row" = table$num_rows == if (empty) 0L else 4L)
  for (i in seq_along(columns)) {
    name <- columns[[i]]; kind <- kinds[[i]]
    field <- table$schema$fields[[i]]
    stopifnot_case(paste(label, "column", name), "the field is nullable" = field$nullable,
      "the field has its kind's Arrow type" = field$type$Equals(arrow_types[[kind]]))
    column <- table[[name]]
    expected <- if (empty) character() else strings(truth$values[[name]])
    if (kind == "float") {
      actual <- float_hex(column$as_vector())
    } else if (kind == "date") {
      actual <- as.character(column$cast(arrow::int32())$as_vector())
      expected <- if (empty) character() else strings(truth$date_days)
    } else if (kind == "datetime") {
      # Inspect the unit and zone above, then the exact stored microseconds.
      # Avoid POSIXct conversion and its machine timezone and double range.
      actual <- column$cast(arrow::int64())$cast(arrow::utf8())$as_vector()
      expected <- if (empty) character() else strings(truth$timestamp_us)
    } else {
      # Casting inside Arrow preserves INT64_MIN, which bit64 uses as NA.
      actual <- column$cast(arrow::utf8())$as_vector()
    }
    expect_identical(actual, expected, paste(label, "column", name, "holds the authored values"))
  }
}

check_csv <- function(saved, empty) {
  expected <- if (empty) paste0(strsplit(truth$csv, "\n", fixed = TRUE)[[1L]][[1L]], "\n") else truth$csv
  label <- paste("the CSV output of", source_label(empty))
  expect_identical(saved, charToRaw(enc2utf8(expected)), paste(label, "is the authored bytes"))
  # CSV is text. Supply an explicit text schema and its documented ingestion
  # semantics: quoted and bare empty fields both read as missing. The independent
  # exact-byte control above also pins their distinct written forms.
  text_schema <- do.call(arrow::schema, setNames(rep(list(arrow::utf8()), length(columns)), columns))
  table <- arrow::read_csv_arrow(arrow::BufferReader$create(saved), col_types = text_schema,
                                  na = "", quoted_na = TRUE, as_data_frame = FALSE)
  expected <- utils::read.csv(text = expected, colClasses = "character", na.strings = "",
                              check.names = FALSE, strip.white = FALSE)
  expect_identical(names(table), columns, paste(label, "has the authored columns"))
  stopifnot_case(label, "arrow reads every row" = table$num_rows == nrow(expected))
  for (name in columns) expect_identical(table[[name]]$as_vector(), expected[[name]],
    paste(label, "column", name, "reads as the authored text"))
}

check_submission <- function(directory, profile, empty) {
  document <- yamaa_project(file.path(directory, "project.yaml"))$submission("adam", created = truth$created)
  label <- sprintf("the submission of the %s output of %s", profile, source_label(empty))
  if (empty) {
    # A transported dataset without rows must declare it has no data.
    expect_identical(document$issues$condition, "transport_dataset_empty", paste(label, "is refused"))
    failed <- tryCatch({document$save(); FALSE}, yamaa_error = function(error) TRUE)
    stopifnot_case(label, "its save is refused" = failed,
      "nothing is written" = !any(file.exists(file.path(directory, c("test.json", "define.xml")))))
    return(invisible(NULL))
  }
  stopifnot_case(label, "the submission has no issues" = nrow(document$issues) == 0L)
  unlink(file.path(directory, c("spec.yaml", "project.yaml", paste0("output.", profile))))
  stopifnot_case(label, "the submission saves" = returns_paths(document$save()))
  saved <- rawfile(file.path(directory, "test.json"))
  # A repeated save rewrites the held bytes, so a changed file is restored.
  writeBin(charToRaw("sentinel"), file.path(directory, "test.json"))
  stopifnot_case(label, "the submission saves again" = returns_paths(document$save()))
  expect_identical(rawfile(file.path(directory, "test.json")), saved, paste(label, "rewrites the held Dataset-JSON"))
  # bigint_as_char is required: R doubles cannot carry every signed-i64 value.
  data <- jsonlite::fromJSON(rawToChar(saved), simplifyVector = FALSE, bigint_as_char = TRUE)
  row_lines <- grep("^    \\[", strsplit(rawToChar(saved), "\n", fixed = TRUE)[[1L]], value = TRUE)
  expect_identical(data[c("datasetJSONVersion", "datasetJSONCreationDateTime")],
    list(datasetJSONVersion = "1.1.0", datasetJSONCreationDateTime = truth$created),
    paste(label, "states its version and creation time"))
  expect_identical(vapply(data$columns, `[[`, character(1), "name"), columns, paste(label, "names the columns"))
  expect_identical(vapply(data$columns, `[[`, character(1), "dataType"),
    c("string", "string", "integer", "float", "date", "datetime", "string"), paste(label, "types the columns"))
  stopifnot_case(label, "it counts every record" = data$records == if (empty) 0L else 4L,
    "it holds every row" = length(data$rows) == if (empty) 0L else 4L,
    "every row has every column" = all(vapply(data$rows, length, integer(1)) == length(columns)))
  for (i in seq_along(columns)) {
    kind <- kinds[[i]]; name <- columns[[i]]
    values <- lapply(data$rows, `[[`, i)
    expected <- if (empty) character() else strings(truth$values[[name]])
    if (kind == "float") {
      stopifnot_case(paste(label, "column", name), "a float is a JSON number or null" =
        all(vapply(values, function(value) is.null(value) || is.numeric(value), logical(1))))
      actual <- float_hex(vapply(values, function(value) if (is.null(value)) NA_real_ else
        as.double(value), double(1)))
      if (!empty) {
        # jsonlite represents the JSON integer token -0 as R integer zero. Its
        # mathematical value is correct; separately pin the original signed token
        # and require exact binary64 bits for every other float and in Parquet.
        stopifnot_case(paste(label, "column", name), "the first row keeps the token -0" =
          grepl(", -0,", row_lines[[1L]], fixed = TRUE))
        expected[[1L]] <- "0000000000000000"
      }
    } else {
      actual <- strings(values)
      if (kind == "int" && !empty) {
        # bigint_as_char preserves exact numeric tokens as text. Also assert
        # their unquoted wire form so JSON strings cannot masquerade as numbers.
        for (row in seq_along(expected)) if (!is.na(expected[[row]])) {
          stopifnot_case(sprintf("%s column %s row %d", label, name, row), "the integer is an unquoted token" =
            grepl(paste0(", ", expected[[row]], ","), row_lines[[row]], fixed = TRUE))
        }
      }
      if (profile == "csv" && name == "EMPTY" && !empty) expected <- rep(NA_character_, 4L)
    }
    expect_identical(actual, expected, paste(label, "column", name, "holds the authored values"))
  }
  xml <- rawToChar(rawfile(file.path(directory, "define.xml")))
  stopifnot_case(label, "Define-XML links the Dataset-JSON" = grepl('xlink:href="test.json"', xml, fixed = TRUE))
  for (i in seq_along(columns)) {
    # Full immutable XML goldens are owned by public_define.R. This control also
    # links every new typed JSON column to its corresponding saved XML item.
    stopifnot_case(paste(label, "column", columns[[i]]), "Define-XML has the column's ItemDef" =
      grepl(paste0('OID="', data$columns[[i]]$itemOID, '" Name="', columns[[i]], '"'), xml, fixed = TRUE))
  }
}

run_saved_readers <- function() {
  directory <- tempfile("saved-readers-"); dir.create(directory)
  on.exit(unlink(directory, recursive = TRUE), add = TRUE)
  for (profile in c("csv", "parquet")) for (empty in c(FALSE, TRUE)) {
    spec <- truth$specification
    spec$output$path <- paste0("output.", profile)
    entry <- file.path(directory, "spec.yaml")
    writeLines(jsonlite::toJSON(spec, auto_unbox = TRUE, null = "null"), entry, useBytes = TRUE)
    # The fixture's study, codelists and sdtm section form the project file.
    writeLines(jsonlite::toJSON(truth$environment, auto_unbox = TRUE), file.path(directory, "project.yaml"), useBytes = TRUE)
    input <- if (empty) paste0(strsplit(truth$input, "\n", fixed = TRUE)[[1L]][[1L]], "\n") else truth$input
    writeBin(charToRaw(enc2utf8(input)), file.path(directory, "input.csv"))
    built <- yamaa_spec(entry)$run()
    label <- sprintf("a %s run of %s", profile, source_label(empty))
    stopifnot_case(label, "the run has no issues" = nrow(built$issues) == 0L)
    output <- file.path(directory, paste0("output.", profile))
    stopifnot_case(label, "nothing is written before save" = !file.exists(output))
    original <- rawfile(entry)
    unlink(file.path(directory, c("spec.yaml", "input.csv")))
    stopifnot_case(label, "the run saves" = returns_paths(built$save()))
    saved <- rawfile(output)
    for (attempt in 1:2) {
      if (profile == "csv") check_csv(saved, empty) else check_parquet(saved, empty)
      stopifnot_case(sprintf("%s, save %d", label, attempt), "the run saves again" = returns_paths(built$save()),
        "it rewrites the same bytes" = identical(rawfile(output), saved))
    }
    writeBin(original, entry)
    check_submission(directory, profile, empty)
    unlink(file.path(directory, c("test.json", "define.xml")))
  }
}

run_saved_readers()
cat("Installed R saved artifact readers: arrow", as.character(utils::packageVersion("arrow")),
    "and jsonlite", as.character(utils::packageVersion("jsonlite")), "passed\n")
