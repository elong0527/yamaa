# Host package metadata only. The Rust activation port supplies captured lock
# versions and statically admitted callable names; no project code is imported.
.yamaa_verify_locked_versions <- function(locked_versions, calls,
                                         version = utils::packageVersion,
                                         base_packages = NULL) {
  if (!length(calls)) return(list())
  packages <- sub("::.*$", "", calls)
  if (is.null(base_packages)) {
    # Each called package's own DESCRIPTION says whether it is a base package,
    # without installed.packages(), which reads every installed package.
    base_packages <- packages[vapply(packages, function(package) identical(
      suppressWarnings(utils::packageDescription(package, fields = "Priority")),
      "base"), logical(1))]
  }
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
        is.na(expected)) {
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
    if (length(actual) != 1L || is.na(actual)) {
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
  if (length(call) != 1L || !is.character(call) || is.na(call)) {
    stop("expected one callable name", call. = FALSE)
  }
  parts <- strsplit(call, "::", fixed = TRUE)[[1L]]
  if (length(parts) != 2L || any(!nzchar(parts))) {
    stop("callable must be package-qualified", call. = FALSE)
  }
  target <- lookup(parts[[1L]], parts[[2L]])
  if (!is.function(target)) stop("installed member is not callable", call. = FALSE)
  signature <- if (is.primitive(target)) args(target) else target
  if (!is.function(signature)) {
    stop("installed callable has no concrete call signature", call. = FALSE)
  }
  actual <- formals(signature)
  names <- names(actual)
  if (anyDuplicated(parameters) || "..." %in% names ||
      !setequal(names, parameters)) {
    stop("installed callable signature differs from params", call. = FALSE)
  }
  target
}

# Mechanical host capabilities. Native Rust decides when each may run.
# A bound function receives plain R values named by its parameters and replies
# with the plain value it returned; Rust converts that value by the declared
# result type.
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
      .utf8_raw(finding$package), .utf8_raw(finding$reason),
      if (is.null(finding$expected)) NULL else .utf8_raw(finding$expected),
      if (is.null(finding$actual)) NULL else .utf8_raw(finding$actual)))
  })
  resolve <- function(call, parameters) reply(function() {
    target <- .yamaa_resolve_locked_function(text(call), vapply(parameters, text, ""))
    force(target)
    function(arguments) reply(function() do.call(target, arguments, quote = TRUE))
  })
  list(verify = verify, resolve = resolve)
}

# Caller-supplied registry resolution: the registry is a named list or an
# environment mapping the full logical call string to a function. A missing
# binding is a refusal; there is no fallback to namespace resolution, and
# declared-lock auditing stays a separate, optional step.
.yamaa_resolve_registry_function <- function(registry, call, parameters) {
  if (length(call) != 1L || !is.character(call) || is.na(call)) {
    stop("expected one callable name", call. = FALSE)
  }
  if (!is.environment(registry) && anyDuplicated(names(registry))) {
    stop("registry names are duplicated", call. = FALSE)
  }
  target <- if (is.environment(registry)) {
    if (exists(call, envir = registry, inherits = FALSE)) {
      get(call, envir = registry, inherits = FALSE)
    }
  } else {
    registry[[call, exact = TRUE]]
  }
  if (is.null(target)) stop("registered binding is missing", call. = FALSE)
  if (!is.function(target)) stop("registered member is not callable", call. = FALSE)
  signature <- if (is.primitive(target)) args(target) else target
  if (!is.function(signature)) {
    stop("registered callable has no concrete call signature", call. = FALSE)
  }
  actual <- formals(signature)
  names <- names(actual)
  if (anyDuplicated(parameters) || "..." %in% names ||
      !setequal(names, parameters)) {
    stop("registered callable signature differs from params", call. = FALSE)
  }
  target
}

# Vectorized batch call over named columns: every column must have the
# same length and the result must have exactly that length. The shared
# engine re-validates length and exact result types before accepting any
# output; this helper refuses ragged columns and wrong-length results.
.yamaa_call_batch <- function(target, columns) {
  lengths <- lengths(columns)
  if (length(unique(lengths)) > 1L) stop("batch columns differ in length", call. = FALSE)
  rows <- if (length(lengths)) lengths[[1L]] else 0L
  returned <- do.call(target, columns)
  if (length(returned) != rows) stop("batch callable returned the wrong length", call. = FALSE)
  returned
}
