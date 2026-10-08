library(yamaa)
# Raw source bytes (hex encoding, not a digest) and expected JSON are shared
# independent truth. Base R suffices: no host YAML or JSON parser participates.
truth <- read.delim(system.file("yaml_transport.tsv", package = "yamaa"),
  sep = "\t", quote = "", comment.char = "", colClasses = "character",
  fileEncoding = "UTF-8", check.names = FALSE)
before <- loadedNamespaces()
for (i in seq_len(nrow(truth))) {
  hex <- truth$source_hex[i]
  source <- if (!nzchar(hex)) raw() else {
    starts <- seq.int(1L, nchar(hex), by = 2L)
    as.raw(strtoi(substring(hex, starts, starts + 1L), base = 16L))
  }
  for (attempt in 1:2) {
    stopifnot(identical(decode_yaml(source), truth$expected[i]))
  }
}
stopifnot(!"yaml12" %in% setdiff(loadedNamespaces(), before))
# An escaped NUL stays inside JSON string syntax; it never crosses an R
# character conversion as an embedded NUL or gets dropped from the value.
stopifnot(identical(decode_yaml(as.raw(c(34L, 92L, 48L, 34L))),
  '{"outcome":{"document":{"nodes":[{"kind":"text","value":"\\u0000"}],"root":0},"locations":[{"column":1,"line":1,"offset":0}],"status":"decoded"},"protocol":"yaml/1"}'))
for (source in list("1", 1, TRUE, NULL, list(),
  structure(as.raw(49), names = "x"), structure(as.raw(49), class = "marked"))) {
  stopifnot(inherits(tryCatch(decode_yaml(source), error = identity), "error"))
}
stopifnot(identical(decode_yaml(raw(8388609L)),
  '{"outcome":{"limit":8388608,"phase":"yaml_source","resource":"source_bytes","status":"resource_limit"},"protocol":"yaml/1"}'))
stopifnot(identical(engine_info()$execution_supported, FALSE))
