library(yamaanative)
info <- engine_info()
stopifnot(
  identical(info$core_version, "0.1.0"),
  identical(info$protocol_version, "installation-probe/1"),
  identical(info$execution_supported, FALSE),
  identical(info$installation_resource, "yamaa native installation probe\n"),
  identical(info, engine_info())
)
