# Interpret the canonical submission schemas rather than maintaining a second
# closed vocabulary. Installed copies are checked against yaml/ in tests.
.submission_schema <- function(schema_root = NULL) {
  if (is.null(schema_root)) schema_root <- system.file("submission", package = "cdiscbuilder")
  if (!nzchar(schema_root)) stop("pass the canonical YAML schema_root")
  schema <- list()
  for (name in c("schema_shared.yaml", "schema_metadata.yaml", "schema_define.yaml")) {
    schema <- modifyList(schema, yaml12::read_yaml(file.path(schema_root, name), simplify = FALSE))
  }
  schema
}

.submission_shape <- function(value, type, schema, path) {
  fail <- function(condition, context = list()) list(
    .submission_diagnostic(condition, path, "REQ-0907", context))
  if (length(type) > 1L) {
    alternatives <- lapply(type, function(alternative) .submission_shape(value, alternative, schema, path))
    if (any(lengths(alternatives) == 0L)) return(list())
    return(alternatives[[which.min(lengths(alternatives))]])
  }
  type <- as.character(type)
  if (grepl("^list\\[", type)) {
    if (!is.list(value) || !is.null(names(value))) return(fail("invalid_field_type"))
    child_type <- sub("^list\\[(.*)\\]$", "\\1", type)
    diagnostics <- list()
    for (i in seq_along(value)) diagnostics <- c(diagnostics,
      .submission_shape(value[[i]], child_type, schema, paste0(path, "[", i - 1L, "]")))
    return(diagnostics)
  }
  if (type %in% c("str", "int", "float", "bool", "null")) {
    valid <- switch(type,
      str = is.character(value) && length(value) == 1L,
      int = .submission_is_integer(value),
      float = is.numeric(value) && length(value) == 1L && is.finite(value),
      bool = is.logical(value) && length(value) == 1L && !is.na(value),
      null = is.null(value))
    if (!valid) return(fail("invalid_field_type", list(type = type)))
    return(list())
  }
  definition <- schema[[type]]
  if (is.null(definition)) stop("unknown submission schema type: ", type)
  if (!is.null(definition$type)) {
    diagnostics <- .submission_shape(value, definition$type, schema, path)
    if (length(diagnostics)) return(diagnostics)
    if (!is.null(definition$values) && !(value %in% unlist(definition$values))) {
      return(fail("value_not_permitted", list(value = value, permitted = definition$values)))
    }
    if (!is.null(definition$min_length) && nchar(value, type = "chars") < definition$min_length) {
      return(fail("value_not_permitted", list(value = value)))
    }
    if (!is.null(definition$pattern) && !grepl(definition$pattern, value, perl = TRUE)) {
      return(fail("value_not_permitted", list(value = value)))
    }
    return(list())
  }
  if (!is.list(value) || (length(value) && is.null(names(value)))) return(fail("invalid_field_type"))
  fields <- unlist(lapply(definition, names))
  diagnostics <- list()
  for (key in setdiff(names(value), fields)) diagnostics <- c(diagnostics,
    list(.submission_diagnostic("unknown_field", paste0(path, ".", key), "REQ-0907", list(field = key))))
  for (field in definition) {
    key <- names(field)[1L]
    properties <- field[[key]]
    child_path <- paste0(path, ".", key)
    if (!(key %in% names(value))) {
      if (isTRUE(properties$required)) diagnostics <- c(diagnostics,
        list(.submission_diagnostic("missing_required_field", child_path, "REQ-0907", list(field = key))))
      next
    }
    found <- .submission_shape(value[[key]], properties$type, schema, child_path)
    if (!length(found)) {
      if (!is.null(properties$values) && !(value[[key]] %in% unlist(properties$values))) {
        found <- list(.submission_diagnostic("value_not_permitted", child_path, "REQ-0907", list(value = value[[key]])))
      }
      if (!is.null(properties$min_length) && nchar(value[[key]]) < properties$min_length) {
        found <- list(.submission_diagnostic("value_not_permitted", child_path, "REQ-0907", list(value = value[[key]])))
      }
    }
    diagnostics <- c(diagnostics, found)
  }
  diagnostics
}

.submission_spec_shape <- function(spec, schema) {
  diagnostics <- list()
  if (!is.null(spec$submission)) diagnostics <- .submission_shape(spec$submission,
    "submission_dataset_class", schema, "submission")
  for (column in spec$columns) {
    if (!is.null(column$submission)) diagnostics <- c(diagnostics,
      .submission_shape(column$submission, "submission_column_class", schema,
                        paste0("columns.", column$name, ".submission")))
  }
  for (row in spec$rows) {
    for (name in names(row$submission)) diagnostics <- c(diagnostics,
      .submission_shape(row$submission[[name]], "submission_column_class", schema,
                        paste0("rows.", row$id, ".submission.", name)))
  }
  diagnostics
}

.submission_normalize <- function(value, type, schema) {
  if (length(type) > 1L) {
    for (alternative in type) {
      if (!length(.submission_shape(value, alternative, schema, ""))) {
        return(.submission_normalize(value, alternative, schema))
      }
    }
    return(value)
  }
  type <- as.character(type)
  if (grepl("^list\\[", type)) {
    child_type <- sub("^list\\[(.*)\\]$", "\\1", type)
    return(lapply(value, .submission_normalize, type = child_type, schema = schema))
  }
  if (type %in% c("str", "int", "float", "bool", "null")) return(value)
  definition <- schema[[type]]
  if (!is.null(definition$type)) return(.submission_normalize(value, definition$type, schema))
  for (field in definition) {
    key <- names(field)[1L]
    properties <- field[[key]]
    if (!(key %in% names(value)) && "default" %in% names(properties)) {
      value[[key]] <- properties$default
    }
    if (key %in% names(value)) value[[key]] <- .submission_normalize(value[[key]], properties$type, schema)
  }
  value
}
