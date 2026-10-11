library(yamaa)
reject <- function(expr, pattern = NULL) {
  error <- tryCatch(force(expr), error = identity)
  stopifnot(inherits(error, "error"))
  if (!is.null(pattern)) stopifnot(grepl(pattern, conditionMessage(error), fixed = TRUE))
  invisible(error)
}
for (text in c("-9223372036854775808", "9223372036854775807", "9007199254740993", "-2147483648", "0")) {
  x <- int64(text)
  stopifnot(identical(as.character(x), text), identical(is.na(x), FALSE), identical(length(x), 1L))
  invisible(capture.output(print(x)))
}
stopifnot(
  identical(as.character(int64(1L)), "1"),
  identical(as.character(int64("9007199254740993") + 1L), "9007199254740994"),
  identical(as.character(1L + int64("9007199254740993")), "9007199254740994"),
  identical(as.character(-int64("5")), "-5"),
  identical(as.character(+int64("5")), "5"),
  identical(as.character(int64("5") - 2L), "3"),
  identical(as.character(int64("5") * 2L), "10"),
  identical(int64("3") / 2L, 1.5),
  identical(int64("1") + 0.5, 1.5),
  identical(int64("9007199254740993") > 9007199254740992, TRUE),
  identical(int64("9007199254740993") == 9007199254740992, FALSE),
  identical(int64("3") >= 3L, TRUE), identical(int64("3") <= 3L, TRUE),
  identical(int64("3") != 4L, TRUE), identical(int64("3") < 4L, TRUE),
  identical(int64("3") + NA, NA), identical(int64("3") == NA, NA),
  identical(int64("3") + Inf, NA), identical(int64("3") + NaN, NA),
  identical(as.double(int64("9007199254740992")), 9007199254740992),
  identical(as.double(int64("-9223372036854775808")), -2^63),
  identical(as.integer(int64("2147483647")), 2147483647L),
  identical(as.integer(int64("-2147483647")), -2147483647L)
)
for (bad in list(1, NA_integer_, NA_character_, character(), c("1","2"), "01", "+1", "-0", " 1", "1.0", "9223372036854775808", "-9223372036854775809")) reject(int64(bad))
reject(int64("9223372036854775807") + 1L, "9223372036854775808")
reject(-int64("-9223372036854775808"), "9223372036854775808")
reject(int64("3") / 0L, "division_by_zero")
reject(int64("1") + TRUE, "incompatible scalar operand")
reject(int64("1") + c(1L, 2L), "expected one supported scalar")
reject(int64("1") == "1", "incompatible scalar operands")
reject(int64("1") ^ 2L, "unsupported scalar operator")
reject(int64("3") %% 2L, "unsupported scalar operator")
reject(as.double(int64("9007199254740993")), "not exactly representable")
reject(as.integer(int64("-2147483648")), "not representable")
reject(as.integer(int64("2147483648")), "not representable")

for (bytes in list(raw(), charToRaw("a"), as.raw(c(97,0,98)), as.raw(c(0xe9,0x9b,0xaa,0xf0,0x9f,0xa6,0x80)))) {
  x <- utf8_scalar(bytes)
  stopifnot(identical(utf8_bytes(x), bytes), identical(length(x), 1L), identical(is.na(x), FALSE), x == utf8_scalar(bytes))
  invisible(capture.output(print(x)))
  kept <- x
  rm(x)
  invisible(gc())
  for (i in seq_len(20)) stopifnot(identical(utf8_bytes(kept), bytes))
}
stopifnot(identical(as.character(utf8_scalar(raw())), ""),
          identical(as.character(utf8_scalar("text")), "text"),
          utf8_scalar("a") < "b", "a" < utf8_scalar("b"),
          utf8_scalar("e\u0301") != utf8_scalar("\u00e9"))
original <- utf8_scalar(as.raw(c(97, 0, 98)))
changed <- utf8_bytes(original)
changed[1L] <- as.raw(122L)
stopifnot(identical(utf8_bytes(original), as.raw(c(97, 0, 98))),
          identical(writeBin(int64("0") * -0.0, raw(), size = 8L, endian = "little"),
                    as.raw(c(rep(0L, 7L), 128L))))
latin <- rawToChar(as.raw(0xe9)); Encoding(latin) <- "latin1"
stopifnot(identical(utf8_bytes(utf8_scalar(latin)), as.raw(c(0xc3,0xa9))))
byte_text <- rawToChar(as.raw(0xe9)); Encoding(byte_text) <- "bytes"
reject(utf8_scalar(byte_text), "byte-marked")
invalid_text <- rawToChar(as.raw(0xff)); Encoding(invalid_text) <- "UTF-8"
reject(utf8_scalar(invalid_text), "invalid UTF-8")
for (bytes in list(as.raw(255), as.raw(c(0xc0,0x80)), as.raw(c(0xed,0xa0,0x80)), as.raw(c(0xf4,0x90,0x80,0x80)))) reject(utf8_scalar(bytes), "invalid UTF-8")
reject(as.character(utf8_scalar(as.raw(c(97,0,98)))), "cannot preserve NUL")
reject(utf8_bytes(1L), "expected a text scalar")
reject(utf8_scalar(NA_character_))
reject(utf8_scalar(as.raw(rep(1L, 1048577L))), "exceeds byte limit")
for (fake in list(structure("bad", class="yamaa_int64"), structure(c("1","2"),class="yamaa_int64"), structure(1, class="yamaa_int64"), structure("1",class=c("yamaa_int64","other")), structure("1",class="yamaa_int64", names="x"))) reject(as.character(fake))
for (fake in list(structure(as.raw(255), class="yamaa_utf8"), structure("text",class="yamaa_utf8"), structure(raw(),class=c("yamaa_utf8","other")))) reject(utf8_bytes(fake))
cat("Lossless R scalar contracts passed\n")


# Exercise conversions on actual public output, including separate missingness.
check_integer_output_conversions <- function() {
  work <- tempfile("integer-output-"); dir.create(work)
  on.exit(unlink(work, recursive = TRUE))
  entry <- file.path(work, "spec.yaml")
  writeLines(c(
    "schema_version: '1.0'", "domain: TEST", "keys: [ID]",
    "input: {SRC: input.csv}",
    "output: {path: output.csv, columns: [ID, VALUE]}", "columns:",
    "  - {name: ID, type: int, derivation: {source: SRC.ID}}",
    "  - {name: VALUE, type: int, derivation: {source: SRC.VALUE}}"
  ), entry)
  truth <- c("-9223372036854775808", "9223372036854775807",
             "9007199254740993", "-9007199254740993",
             "9007199254740992", "-9007199254740992",
             "-2147483648", "-2147483647", "2147483647", NA_character_)
  rows <- paste(seq_along(truth), ifelse(is.na(truth), "", truth), sep = ",")
  writeBin(charToRaw(paste0("ID,VALUE\n", paste(rows, collapse = "\n"), "\n")),
           file.path(work, "input.csv"))
  result <- yamaa_domain(entry)
  stopifnot(nrow(result$issues) == 0L)
  output <- result$output$VALUE
  stopifnot(identical(class(output), "yamaa_int64_vector"),
            identical(as.character(output), truth),
            identical(is.na(output), c(rep(FALSE, 9L), TRUE)))
  exact <- output[c(1L, 5L, 6L, 10L)]
  exact_truth <- c(-2^63, 2^53, -2^53, NA_real_)
  stopifnot(identical(as.double(exact), exact_truth),
            identical(as.numeric(exact), exact_truth),
            identical(as.integer(output[c(8L, 9L, 10L)]),
                      c(-2147483647L, 2147483647L, NA_integer_)))
  for (index in c(2L, 3L, 4L)) {
    reject(as.double(output[c(8L, index, 10L)]), "not exactly representable")
    reject(as.numeric(output[c(8L, index, 10L)]), "not exactly representable")
  }
  for (index in 1:7) {
    reject(as.integer(output[c(8L, index, 10L)]), "not representable")
  }
  stopifnot(identical(as.character(output), truth),
            identical(as.character(result$output$VALUE), truth))
  named <- setNames(output[c(8L, 9L, 10L)], c("low", "high", "missing"))
  stopifnot(identical(as.double(named),
                      c(low = -2147483647, high = 2147483647, missing = NA_real_)),
            identical(as.integer(named),
                      c(low = -2147483647L, high = 2147483647L, missing = NA_integer_)))
  for (convert in list(as.double, as.integer)) {
    template <- if (identical(convert, as.double)) NA_real_ else NA_integer_
    stopifnot(identical(convert(output[10L]), template),
              identical(convert(output[integer()]), template[integer()]))
    for (fake in list(structure(1, class = "yamaa_int64_vector"),
                      structure("1", class = c("yamaa_int64_vector", "other")),
                      structure("1", class = "yamaa_int64_vector", dim = c(1L, 1L)),
                      structure("01", class = "yamaa_int64_vector"),
                      structure("9223372036854775808", class = "yamaa_int64_vector"))) {
      reject(convert(fake))
    }
  }
  rm(result); unlink(work, recursive = TRUE); invisible(gc())
  for (attempt in seq_len(5L)) {
    stopifnot(identical(as.double(exact), exact_truth),
              identical(as.character(output), truth))
  }
}
check_integer_output_conversions()
cat("Full-range R integer output exact-or-error conversions and ownership passed\n")
