# Shared portable function registry support.
#
# These internal helpers validate the declarative registry and its conformance
# fixtures. Expression parsing and dataset execution remain outside the current
# R package implementation.

.portable_registry_error <- function(condition, ...) {
  context <- list(...)
  details <- paste(
    vapply(
      names(context),
      function(name) paste0(name, "=", paste(context[[name]], collapse = ",")),
      character(1)
    ),
    collapse = "; "
  )
  error <- structure(
    list(
      message = if (nzchar(details)) paste(condition, details, sep = ": ") else condition,
      call = NULL,
      condition = condition,
      context = context
    ),
    class = c("portable_registry_error", "error", "condition")
  )
  stop(error)
}

.portable_require_fields <- function(value, expected, path, optional = character()) {
  if (!is.list(value) || is.null(names(value))) {
    .portable_registry_error("invalid_registry", path = path, reason = "not_mapping")
  }
  actual <- names(value)
  missing <- setdiff(expected, actual)
  unknown <- setdiff(actual, c(expected, optional))
  if (length(missing) > 0 || length(unknown) > 0) {
    .portable_registry_error(
      "invalid_registry",
      path = path,
      missing = missing,
      unknown = unknown
    )
  }
  value
}

.portable_version_key <- function(version) {
  as.integer(strsplit(version, ".", fixed = TRUE)[[1]])
}

.portable_version_before <- function(left, right) {
  left_parts <- .portable_version_key(left)
  right_parts <- .portable_version_key(right)
  length(left_parts) <- max(length(left_parts), length(right_parts))
  length(right_parts) <- length(left_parts)
  left_parts[is.na(left_parts)] <- 0L
  right_parts[is.na(right_parts)] <- 0L
  first_difference <- which(left_parts != right_parts)[1]
  !is.na(first_difference) && left_parts[first_difference] < right_parts[first_difference]
}

.validate_portable_signature <- function(signature, entry_path) {
  signature <- .portable_require_fields(
    signature,
    c("parameters", "min_arity", "max_arity"),
    paste0(entry_path, ".signature")
  )
  parameters <- signature$parameters
  minimum <- signature$min_arity
  maximum <- signature$max_arity
  if (!is.list(parameters) || length(parameters) == 0 ||
      length(minimum) != 1 || !is.numeric(minimum) || minimum < 1) {
    .portable_registry_error("invalid_registry", path = paste0(entry_path, ".signature"))
  }
  if (!is.null(maximum) &&
      (length(maximum) != 1 || !is.numeric(maximum) || maximum < minimum)) {
    .portable_registry_error(
      "invalid_registry",
      path = paste0(entry_path, ".signature.max_arity")
    )
  }

  accepted_types <- c("str", "int", "float", "bool", "date", "datetime", "record_star")
  parameter_names <- character()
  variadic <- FALSE
  for (index in seq_along(parameters)) {
    parameter_path <- paste0(entry_path, ".signature.parameters[", index - 1, "]")
    parameter <- .portable_require_fields(
      parameters[[index]],
      c("name", "types"),
      parameter_path,
      "variadic"
    )
    if (!is.character(parameter$name) || length(parameter$name) != 1 ||
        !grepl("^[a-z][a-z0-9_]*$", parameter$name) ||
        parameter$name %in% parameter_names ||
        !is.character(parameter$types) || length(parameter$types) == 0 ||
        any(!parameter$types %in% accepted_types) || anyDuplicated(parameter$types)) {
      .portable_registry_error("invalid_registry", path = parameter_path)
    }
    parameter_names <- c(parameter_names, parameter$name)
    is_variadic <- isTRUE(parameter$variadic)
    if (!is.null(parameter$variadic) && !is.logical(parameter$variadic)) {
      .portable_registry_error("invalid_registry", path = paste0(parameter_path, ".variadic"))
    }
    if (is_variadic && index != length(parameters)) {
      .portable_registry_error("invalid_registry", path = paste0(parameter_path, ".variadic"))
    }
    variadic <- variadic || is_variadic
  }
  if (variadic) {
    if (!is.null(maximum) || minimum < length(parameters)) {
      .portable_registry_error("invalid_registry", path = paste0(entry_path, ".signature"))
    }
  } else if (minimum != length(parameters) || maximum != length(parameters)) {
    .portable_registry_error("invalid_registry", path = paste0(entry_path, ".signature"))
  }
  invisible(TRUE)
}

.validate_portable_document <- function(document, source) {
  document <- .portable_require_fields(
    document,
    c("registry_version", "namespace", "specification_versions", "entries"),
    source
  )
  if (!is.character(document$registry_version) ||
      !grepl("^[0-9]+[.][0-9]+[.][0-9]+$", document$registry_version) ||
      !is.character(document$namespace) ||
      !grepl("^[a-z][a-z0-9_]*([.][a-z][a-z0-9_]*)*$", document$namespace) ||
      !is.character(document$specification_versions) ||
      length(document$specification_versions) == 0 ||
      any(!grepl("^[0-9]+[.][0-9]+$", document$specification_versions)) ||
      anyDuplicated(document$specification_versions) ||
      !is.list(document$entries) || length(document$entries) == 0) {
    .portable_registry_error("invalid_registry", path = source)
  }

  entry_fields <- c(
    "canonical_name", "aliases", "evaluation_kind", "signature",
    "type_promotion", "result_type", "missing_values", "failures",
    "determinism", "accuracy", "availability", "definition"
  )
  registered_names <- character()
  registered_owners <- character()
  for (index in seq_along(document$entries)) {
    entry_path <- paste0(source, ".entries[", index - 1, "]")
    entry <- .portable_require_fields(document$entries[[index]], entry_fields, entry_path)
    if (!is.character(entry$canonical_name) || length(entry$canonical_name) != 1 ||
        !grepl("^[A-Z][A-Z0-9_]*$", entry$canonical_name) ||
        (!is.character(entry$aliases) && length(entry$aliases) != 0) ||
        any(!grepl("^[A-Z][A-Z0-9_]*$", entry$aliases)) ||
        anyDuplicated(entry$aliases) ||
        !entry$evaluation_kind %in% c("scalar", "reducer")) {
      .portable_registry_error("invalid_registry", path = entry_path)
    }
    candidates <- c(entry$canonical_name, entry$aliases)
    collision <- which(candidates %in% registered_names)[1]
    if (!is.na(collision)) {
      name <- candidates[[collision]]
      .portable_registry_error(
        "name_collision",
        namespace = document$namespace,
        name = name,
        first = registered_owners[match(name, registered_names)],
        second = entry$canonical_name
      )
    }
    registered_names <- c(registered_names, candidates)
    registered_owners <- c(
      registered_owners,
      rep(entry$canonical_name, length(candidates))
    )

    .validate_portable_signature(entry$signature, entry_path)
    if (!entry$type_promotion %in% c(
      "preserve_numeric", "promote_numeric", "always_float", "count",
      "preserve_input"
    ) || !entry$result_type %in% c(
      "promoted_numeric", "input_numeric", "float", "int", "input"
    ) || !entry$missing_values %in% c(
      "propagate", "ignore_missing_all_missing", "first_non_missing",
      "null_if_equal", "count_non_missing_or_records"
    ) || !entry$determinism %in% c(
      "binary64", "exact_or_binary64", "order_independent"
    ) || !is.character(entry$definition) || length(entry$definition) != 1 ||
      !nzchar(trimws(entry$definition))) {
      .portable_registry_error("invalid_registry", path = entry_path)
    }
    promotion_results <- list(
      preserve_numeric = c("promoted_numeric", "input_numeric"),
      promote_numeric = "promoted_numeric",
      always_float = "float",
      count = "int",
      preserve_input = "input"
    )
    if (!entry$result_type %in% promotion_results[[entry$type_promotion]]) {
      .portable_registry_error(
        "invalid_registry",
        path = paste0(entry_path, ".result_type"),
        promotion = entry$type_promotion
      )
    }

    failures <- .portable_require_fields(
      entry$failures,
      c("domain", "overflow", "non_finite_result"),
      paste0(entry_path, ".failures")
    )
    if ((!is.character(failures$domain) && length(failures$domain) != 0) ||
        any(!grepl("^[a-z][a-z0-9_]*$", failures$domain)) ||
        anyDuplicated(failures$domain) ||
        !identical(failures$overflow, "fail") ||
        !identical(failures$non_finite_result, "fail")) {
      .portable_registry_error("invalid_registry", path = paste0(entry_path, ".failures"))
    }

    accuracy <- .portable_require_fields(
      entry$accuracy,
      c("mode", "absolute_tolerance", "relative_tolerance"),
      paste0(entry_path, ".accuracy")
    )
    if (!accuracy$mode %in% c("exact", "binary64", "exact_or_binary64", "absolute_or_relative") ||
        !is.numeric(accuracy$absolute_tolerance) || accuracy$absolute_tolerance < 0 ||
        !is.finite(accuracy$absolute_tolerance) ||
        !is.numeric(accuracy$relative_tolerance) || accuracy$relative_tolerance < 0 ||
        !is.finite(accuracy$relative_tolerance)) {
      .portable_registry_error("invalid_registry", path = paste0(entry_path, ".accuracy"))
    }

    availability <- .portable_require_fields(
      entry$availability,
      c("since", "deprecated"),
      paste0(entry_path, ".availability")
    )
    if (!is.character(availability$since) ||
        !grepl("^[0-9]+[.][0-9]+$", availability$since) ||
        (!is.null(availability$deprecated) &&
         (!is.character(availability$deprecated) ||
          !grepl("^[0-9]+[.][0-9]+$", availability$deprecated)))) {
      .portable_registry_error("invalid_registry", path = paste0(entry_path, ".availability"))
    }
  }
  document
}

.portable_read_registry <- function(path) {
  text <- paste(readLines(path, warn = FALSE, encoding = "UTF-8"), collapse = "\n")
  if (grepl("(^|[[:space:]])[&*!][A-Za-z0-9_]", text, perl = TRUE) ||
      grepl("^[[:space:]]*<<:", text, perl = TRUE)) {
    .portable_registry_error("invalid_registry", path = path, reason = "prohibited_yaml")
  }
  tryCatch(
    yaml::yaml.load(text, eval.expr = FALSE),
    error = function(error) {
      .portable_registry_error("invalid_registry", path = path, reason = conditionMessage(error))
    }
  )
}

.portable_entry_index <- function(document) {
  index <- list()
  for (entry in document$entries) {
    for (name in c(entry$canonical_name, entry$aliases)) {
      index[[name]] <- entry
    }
  }
  index
}

.load_portable_registry <- function(core_path, extension_paths = character()) {
  core <- .validate_portable_document(.portable_read_registry(core_path), "core")
  if (!identical(core$namespace, "core")) {
    .portable_registry_error("invalid_registry", path = "core.namespace", expected = "core")
  }
  extensions <- list()
  for (index in seq_along(extension_paths)) {
    document <- .validate_portable_document(
      .portable_read_registry(extension_paths[[index]]),
      paste0("extensions[", index - 1, "]")
    )
    namespace <- document$namespace
    if (identical(namespace, "core") || namespace %in% names(extensions)) {
      .portable_registry_error("namespace_collision", namespace = namespace)
    }
    extensions[[namespace]] <- document
  }
  indexes <- list(core = .portable_entry_index(core))
  for (namespace in names(extensions)) {
    indexes[[namespace]] <- .portable_entry_index(extensions[[namespace]])
  }
  structure(
    list(core = core, extensions = extensions, indexes = indexes),
    class = "portable_registry"
  )
}

.validate_portable_call <- function(
  registry,
  name,
  evaluation_kind,
  argument_types,
  specification_version = "1.0",
  declared_extensions = list()
) {
  parts <- strsplit(name, "::", fixed = TRUE)[[1]]
  if (length(parts) == 2) {
    namespace <- parts[[1]]
    local_name <- parts[[2]]
    required_version <- declared_extensions[[namespace]]
    extension <- registry$extensions[[namespace]]
    if (is.null(required_version) || is.null(extension) ||
        !identical(required_version, extension$registry_version) ||
        !specification_version %in% extension$specification_versions) {
      .portable_registry_error(
        "unavailable_extension",
        namespace = namespace,
        required_version = required_version
      )
    }
  } else if (length(parts) == 1) {
    namespace <- "core"
    local_name <- parts[[1]]
    if (!specification_version %in% registry$core$specification_versions) {
      .portable_registry_error(
        "unavailable_function",
        name = name,
        specification_version = specification_version
      )
    }
  } else {
    .portable_registry_error("unknown_function", name = name)
  }
  entry <- registry$indexes[[namespace]][[toupper(local_name)]]
  if (is.null(entry)) {
    .portable_registry_error("unknown_function", name = name)
  }
  if (!identical(entry$evaluation_kind, evaluation_kind)) {
    .portable_registry_error(
      "wrong_evaluation_kind",
      name = name,
      expected = entry$evaluation_kind,
      actual = evaluation_kind
    )
  }
  if (.portable_version_before(specification_version, entry$availability$since)) {
    .portable_registry_error(
      "unavailable_function",
      name = name,
      specification_version = specification_version
    )
  }
  signature <- entry$signature
  count <- length(argument_types)
  if (count < signature$min_arity ||
      (!is.null(signature$max_arity) && count > signature$max_arity)) {
    .portable_registry_error(
      "wrong_arity",
      name = name,
      actual = count,
      minimum = signature$min_arity,
      maximum = signature$max_arity
    )
  }
  parameters <- signature$parameters
  for (index in seq_along(argument_types)) {
    parameter <- parameters[[min(index, length(parameters))]]
    if (!identical(argument_types[[index]], "null") &&
        !argument_types[[index]] %in% parameter$types) {
      .portable_registry_error(
        "incompatible_type",
        name = name,
        argument = index,
        parameter = parameter$name,
        actual = argument_types[[index]],
        accepted = parameter$types
      )
    }
  }
  entry
}

.evaluate_portable <- function(entry, arguments) {
  name <- entry$canonical_name
  result <- if (identical(entry$evaluation_kind, "reducer")) {
    .evaluate_portable_reducer(name, arguments[[1]])
  } else {
    .evaluate_portable_scalar(name, arguments)
  }
  if (is.numeric(result) && length(result) == 1 && !is.na(result) && !is.finite(result)) {
    .portable_registry_error("non_finite_result", name = name)
  }
  result
}

.evaluate_portable_scalar <- function(name, arguments) {
  missing <- vapply(arguments, is.null, logical(1))
  if (!name %in% c("COALESCE", "GREATEST", "LEAST", "NULLIF") && any(missing)) {
    return(NULL)
  }
  present <- arguments[!missing]
  if (identical(name, "COALESCE")) return(if (length(present)) present[[1]] else NULL)
  if (identical(name, "GREATEST")) return(if (length(present)) max(unlist(present)) else NULL)
  if (identical(name, "LEAST")) return(if (length(present)) min(unlist(present)) else NULL)
  if (identical(name, "NULLIF")) {
    x <- arguments[[1]]
    y <- arguments[[2]]
    return(if (is.null(x) || (!is.null(y) && isTRUE(x == y))) NULL else x)
  }

  x <- arguments[[1]]
  if (identical(name, "ABS")) return(abs(x))
  if (identical(name, "CEIL")) return(as.numeric(ceiling(x)))
  if (identical(name, "FLOOR")) return(as.numeric(floor(x)))
  if (identical(name, "TRUNC")) return(as.numeric(trunc(x)))
  if (identical(name, "SQRT")) {
    if (x < 0) .portable_registry_error("domain_error", name = name)
    return(sqrt(x))
  }
  if (identical(name, "POWER")) {
    y <- arguments[[2]]
    if ((x == 0 && y < 0) || (x < 0 && y != trunc(y))) {
      .portable_registry_error("domain_error", name = name)
    }
    return(x ^ y)
  }
  if (identical(name, "EXP")) return(exp(x))
  if (identical(name, "LN")) {
    if (x <= 0) .portable_registry_error("domain_error", name = name)
    return(log(x))
  }
  if (identical(name, "MOD")) {
    y <- arguments[[2]]
    if (y == 0) .portable_registry_error("domain_error", name = name)
    return(x - trunc(x / y) * y)
  }
  if (identical(name, "NORMAL_CDF")) return(pnorm(x))
  .portable_registry_error("unknown_function", name = name)
}

.evaluate_portable_reducer <- function(name, values) {
  if (identical(name, "COUNT")) {
    if (length(values) > 0 && all(unlist(values) == "__record__")) return(length(values))
    return(sum(!vapply(values, is.null, logical(1))))
  }
  present <- values[!vapply(values, is.null, logical(1))]
  if (length(present) == 0) return(NULL)
  present <- unlist(present)
  if (identical(name, "SUM")) return(sum(present))
  if (identical(name, "MIN")) return(min(present))
  if (identical(name, "MAX")) return(max(present))
  if (identical(name, "MEAN")) return(sum(present) / length(present))
  .portable_registry_error("unknown_function", name = name)
}

.portable_results_equal <- function(entry, actual, expected) {
  if (is.null(actual) || is.null(expected)) return(is.null(actual) && is.null(expected))
  if (is.numeric(expected)) {
    if (identical(entry$accuracy$mode, "exact")) {
      return(isTRUE(actual == expected))
    }
    tolerance <- max(
      entry$accuracy$absolute_tolerance,
      entry$accuracy$relative_tolerance * max(abs(actual), abs(expected))
    )
    return(abs(actual - expected) <= tolerance)
  }
  identical(actual, expected)
}

.run_portable_conformance <- function(registry_path, fixtures_path) {
  fixture <- .portable_read_registry(fixtures_path)
  .portable_require_fields(
    fixture,
    c("registry_version", "evaluation_cases", "validation_cases"),
    fixtures_path
  )
  registry <- .load_portable_registry(registry_path)
  if (!identical(fixture$registry_version, registry$core$registry_version)) {
    stop("fixture registry version does not match core registry")
  }

  covered <- character()
  for (case in fixture$evaluation_cases) {
    entry <- .validate_portable_call(
      registry,
      case$name,
      case$evaluation_kind,
      case$argument_types,
      if (is.null(case$specification_version)) "1.0" else case$specification_version
    )
    actual <- .evaluate_portable(entry, case$arguments)
    if (!.portable_results_equal(entry, actual, case$expected)) {
      stop(case$id, ": conformance result differs")
    }
    covered <- c(covered, entry$canonical_name)
  }
  required <- vapply(registry$core$entries, `[[`, character(1), "canonical_name")
  if (!setequal(covered, required)) {
    stop("fixture coverage differs: ", paste(setdiff(required, covered), collapse = ", "))
  }

  fixture_directory <- dirname(fixtures_path)
  for (case in fixture$validation_cases) {
    extension_paths <- file.path(fixture_directory, unlist(case$extension_registries))
    result <- tryCatch(
      {
        case_registry <- .load_portable_registry(registry_path, extension_paths)
        .validate_portable_call(
          case_registry,
          case$name,
          case$evaluation_kind,
          case$argument_types,
          if (is.null(case$specification_version)) "1.0" else case$specification_version,
          if (is.null(case$declared_extensions)) list() else case$declared_extensions
        )
        NULL
      },
      portable_registry_error = function(error) error
    )
    if (is.null(case$expected_error)) {
      if (!is.null(result)) stop(case$id, ": unexpected error ", result$condition)
    } else if (is.null(result) || !identical(result$condition, case$expected_error)) {
      actual <- if (is.null(result)) "no error" else result$condition
      stop(case$id, ": expected ", case$expected_error, ", got ", actual)
    }
  }
  invisible(c(
    evaluation_cases = length(fixture$evaluation_cases),
    validation_cases = length(fixture$validation_cases)
  ))
}
