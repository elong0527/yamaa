# Host package metadata only. The Rust activation port supplies captured lock
# versions and statically admitted callable names; no project code is imported.
.yamaa_verify_locked_versions <- function(locked_versions, calls,
                                         version = utils::packageVersion,
                                         base_packages = NULL) {
  if (!length(calls)) return(list())
  if (length(calls) > 1024L) stop("called-function limit", call. = FALSE)
  if (is.null(base_packages)) {
    base_packages <- rownames(utils::installed.packages(priority = "base"))
  }
  packages <- sub("::.*$", "", calls)
  packages <- sort(unique(c("yamaa", packages[!packages %in% base_packages])))
  findings <- list()
  record <- function(package, reason, expected = NULL, actual = NULL) {
    findings[[length(findings) + 1L]] <<- list(
      package = package, reason = reason, expected = expected, actual = actual
    )
  }
  for (package in packages) {
    expected <- locked_versions[[package]]
    if (is.null(expected)) {
      record(package, "version_not_locked")
      next
    }
    if (!is.character(expected) || length(expected) != 1L ||
        is.na(expected) || nchar(expected, type = "bytes") > 2048L) {
      record(package, "invalid_locked_version")
      next
    }
    wanted <- tryCatch(package_version(expected), error = function(e) NULL)
    if (is.null(wanted)) {
      record(package, "invalid_locked_version", expected)
      next
    }
    # Namespace metadata identifies installation without loading project code.
    # A missing installation has a portable finding; other failures/interrupts
    # keep their original condition object for the host port.
    installed <- tryCatch(version(package), error = function(e) {
      if (inherits(e, "packageNotFoundError")) return(NULL)
      stop(e)
    })
    if (is.null(installed)) {
      record(package, "package_not_installed", expected)
      next
    }
    actual <- as.character(installed)
    if (length(actual) != 1L || is.na(actual) ||
        nchar(actual, type = "bytes") > 2048L) {
      record(package, "invalid_installed_version", expected)
      next
    }
    parsed <- tryCatch(package_version(actual), error = function(e) NULL)
    if (is.null(parsed)) {
      record(package, "invalid_installed_version", expected, actual)
    } else if (parsed != wanted) {
      record(package, "version_mismatch", expected, actual)
    }
  }
  findings
}

# Normal installed namespace resolution follows the engine's successful lock gate.
# Signature inspection never runs the function or substitutes host defaults.
.yamaa_resolve_locked_function <- function(call, parameters,
                                           lookup = base::getExportedValue) {
  if (length(call) != 1L || !is.character(call) || is.na(call) ||
      nchar(call, type = "bytes") > 2048L || length(parameters) > 1024L) {
    stop("callable metadata limit", call. = FALSE)
  }
  parts <- strsplit(call, "::", fixed = TRUE)[[1L]]
  if (length(parts) != 2L || any(!nzchar(parts))) {
    stop("callable must be package-qualified", call. = FALSE)
  }
  target <- lookup(parts[[1L]], parts[[2L]])
  if (!is.function(target)) stop("installed member is not callable", call. = FALSE)
  actual <- formals(target)
  if (is.primitive(target)) actual <- formals(args(target))
  names <- names(actual)
  if (anyDuplicated(parameters) || "..." %in% names ||
      !setequal(names, parameters)) {
    stop("installed callable signature differs from params", call. = FALSE)
  }
  target
}

# Mechanical host capabilities. Native Rust decides when each may run.
.yamaa_locked_host_capabilities <- function() {
  text <- function(bytes) {
    value <- rawToChar(bytes)
    Encoding(value) <- "UTF-8"
    value
  }
  reply <- function(operation) tryCatch(list(0L, operation()),
    interrupt = function(condition) list(3L, condition),
    error = function(condition) list(1L, condition))
  verify <- function(versions, calls) reply(function() {
    held <- lapply(versions, text)
    selected <- vapply(calls, text, "")
    findings <- .yamaa_verify_locked_versions(held, selected)
    lapply(findings, function(finding) list(
      .scalar_text_bytes(finding$package), .scalar_text_bytes(finding$reason),
      if (is.null(finding$expected)) NULL else .scalar_text_bytes(finding$expected),
      if (is.null(finding$actual)) NULL else .scalar_text_bytes(finding$actual)))
  })
  resolve <- function(call, parameters) reply(function() {
    target <- .yamaa_resolve_locked_function(text(call), vapply(parameters, text, ""))
    force(target)
    function(encoded) {
      called <- reply(function() do.call(target, lapply(encoded, .scalar_unpack), quote = TRUE))
      if (called[[1L]] != 0L) return(called)
      tryCatch({
        packed <- .function_pack(called[[2L]])
        list(0L, list(packed$tag, packed$payload))
      }, interrupt=function(condition) list(3L, condition),
         error=function(condition) list(2L, condition))
    }
  })
  list(verify = verify, resolve = resolve)
}
