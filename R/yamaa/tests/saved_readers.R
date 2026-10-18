# Independent reader controls run on the installed package, never a source import.
library(yamaa)
stopifnot(requireNamespace("arrow", quietly = TRUE),
          utils::packageVersion("arrow") >= "23.0.1",
          requireNamespace("jsonlite", quietly = TRUE),
          utils::packageVersion("jsonlite") >= "2.0.0")
host_fixture_root <- if (file.exists("host-fixtures/saved-readers.json")) "host-fixtures" else
  system.file("host-fixtures", package = "yamaa", mustWork = TRUE)
truth <- jsonlite::fromJSON(file.path(host_fixture_root, "saved-readers.json"), simplifyVector = FALSE)
stopifnot(identical(truth$version, "1"))
columns <- unlist(truth$columns, use.names = FALSE)
kinds <- unlist(truth$types, use.names = FALSE)
rawfile <- function(path) readBin(path, "raw", n = file.info(path)$size)
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
  stopifnot(identical(names(table), columns), table$num_rows == if (empty) 0L else 4L)
  for (i in seq_along(columns)) {
    name <- columns[[i]]; kind <- kinds[[i]]
    field <- table$schema$fields[[i]]
    stopifnot(field$nullable, field$type$Equals(arrow_types[[kind]]))
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
    stopifnot(identical(actual, expected))
  }
}

check_csv <- function(saved, empty) {
  expected <- if (empty) paste0(strsplit(truth$csv, "\n", fixed = TRUE)[[1L]][[1L]], "\n") else truth$csv
  stopifnot(identical(saved, charToRaw(enc2utf8(expected))))
  # CSV is text. Supply an explicit text schema and its documented ingestion
  # semantics: quoted and bare empty fields both read as missing. The independent
  # exact-byte control above also pins their distinct written forms.
  text_schema <- do.call(arrow::schema, setNames(rep(list(arrow::utf8()), length(columns)), columns))
  table <- arrow::read_csv_arrow(arrow::BufferReader$create(saved), col_types = text_schema,
                                  na = "", quoted_na = TRUE, as_data_frame = FALSE)
  expected <- utils::read.csv(text = expected, colClasses = "character", na.strings = "",
                              check.names = FALSE, strip.white = FALSE)
  stopifnot(identical(names(table), columns), table$num_rows == nrow(expected))
  for (name in columns) stopifnot(identical(table[[name]]$as_vector(), expected[[name]]))
}

check_submission <- function(directory, profile, empty) {
  document <- yamaa_define(file.path(directory, "environment.yaml"), "sdtm", created = truth$created)
  if (empty) {
    # A transported dataset without rows must declare it has no data.
    stopifnot(identical(document$issues$condition, "transport_dataset_empty"))
    failed <- tryCatch({document$save(); FALSE}, yamaa_define_error = function(error) TRUE)
    stopifnot(failed, !file.exists(file.path(directory, c("test.json", "define.xml"))))
    return(invisible(NULL))
  }
  stopifnot(nrow(document$issues) == 0L)
  unlink(file.path(directory, c("spec.yaml", "environment.yaml", paste0("output.", profile))))
  stopifnot(identical(document$save(), TRUE), identical(document$save(), FALSE))
  saved <- rawfile(file.path(directory, "test.json"))
  # bigint_as_char is required: R doubles cannot carry every signed-i64 value.
  data <- jsonlite::fromJSON(rawToChar(saved), simplifyVector = FALSE, bigint_as_char = TRUE)
  row_lines <- grep("^    \\[", strsplit(rawToChar(saved), "\n", fixed = TRUE)[[1L]], value = TRUE)
  stopifnot(identical(data$datasetJSONVersion, "1.1.0"),
            identical(data$datasetJSONCreationDateTime, truth$created),
            data$records == if (empty) 0L else 4L,
            identical(vapply(data$columns, `[[`, character(1), "name"), columns),
            identical(vapply(data$columns, `[[`, character(1), "dataType"),
                      c("string", "string", "integer", "float", "date", "datetime", "string")),
            length(data$rows) == if (empty) 0L else 4L,
            all(vapply(data$rows, length, integer(1)) == length(columns)))
  for (i in seq_along(columns)) {
    kind <- kinds[[i]]; name <- columns[[i]]
    values <- lapply(data$rows, `[[`, i)
    expected <- if (empty) character() else strings(truth$values[[name]])
    if (kind == "float") {
      stopifnot(all(vapply(values, function(value) is.null(value) || is.numeric(value), logical(1))))
      actual <- float_hex(vapply(values, function(value) if (is.null(value)) NA_real_ else
        as.double(value), double(1)))
      if (!empty) {
        # jsonlite represents the JSON integer token -0 as R integer zero. Its
        # mathematical value is correct; separately pin the original signed token
        # and require exact binary64 bits for every other float and in Parquet.
        stopifnot(grepl(", -0,", row_lines[[1L]], fixed = TRUE))
        expected[[1L]] <- "0000000000000000"
      }
    } else {
      actual <- strings(values)
      if (kind == "int" && !empty) {
        # bigint_as_char preserves exact numeric tokens as text. Also assert
        # their unquoted wire form so JSON strings cannot masquerade as numbers.
        for (row in seq_along(expected)) if (!is.na(expected[[row]])) {
          stopifnot(grepl(paste0(", ", expected[[row]], ","), row_lines[[row]], fixed = TRUE))
        }
      }
      if (profile == "csv" && name == "EMPTY" && !empty) expected <- rep(NA_character_, 4L)
    }
    stopifnot(identical(actual, expected))
  }
  xml <- rawToChar(rawfile(file.path(directory, "define.xml")))
  stopifnot(grepl('xlink:href="test.json"', xml, fixed = TRUE))
  for (i in seq_along(columns)) {
    # Full immutable XML goldens are owned by public_define.R. This control also
    # links every new typed JSON column to its corresponding saved XML item.
    stopifnot(grepl(paste0('OID="', data$columns[[i]]$itemOID, '" Name="', columns[[i]], '"'), xml, fixed = TRUE))
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
    writeLines(jsonlite::toJSON(truth$environment, auto_unbox = TRUE), file.path(directory, "environment.yaml"), useBytes = TRUE)
    input <- if (empty) paste0(strsplit(truth$input, "\n", fixed = TRUE)[[1L]][[1L]], "\n") else truth$input
    writeBin(charToRaw(enc2utf8(input)), file.path(directory, "input.csv"))
    built <- yamaa_domain(entry)
    stopifnot(nrow(built$issues) == 0L)
    output <- file.path(directory, paste0("output.", profile))
    stopifnot(!file.exists(output))
    original <- rawfile(entry)
    unlink(file.path(directory, c("spec.yaml", "input.csv")))
    stopifnot(identical(built$save(), TRUE))
    saved <- rawfile(output)
    for (attempt in 1:2) {
      if (profile == "csv") check_csv(saved, empty) else check_parquet(saved, empty)
      stopifnot(identical(built$save(), TRUE), identical(rawfile(output), saved))
    }
    writeBin(original, entry)
    check_submission(directory, profile, empty)
    unlink(file.path(directory, c("test.json", "define.xml")))
  }
}

run_saved_readers()
cat("Installed R saved artifact readers: arrow", as.character(utils::packageVersion("arrow")),
    "and jsonlite", as.character(utils::packageVersion("jsonlite")), "passed\n")
