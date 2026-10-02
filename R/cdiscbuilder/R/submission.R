# Submission validation consumes resolved specifications. Keeping these checks
# independent of derivation lets both R engines use the same submission layer.

.submission_diagnostic <- function(condition, path, requirement, context = list(),
                                   phase = "validation") {
  list(phase = phase, condition = condition, spec_paths = list(path),
       requirement = requirement, context = context)
}

.submission_equal_declarations <- function(left, right) {
  canonical <- function(value) {
    if (!is.list(value)) return(value)
    if (!is.null(names(value))) value <- value[sort(names(value))]
    lapply(value, canonical)
  }
  identical(canonical(left), canonical(right))
}

.submission_checks <- function(column) {
  checks <- column$verifications
  if (is.null(checks)) return(list())
  if (!is.null(names(checks))) return(lapply(names(checks), function(name) {
    setNames(list(checks[[name]]), name)
  }))
  checks
}

.submission_expression <- function(value) {
  if (is.character(value)) return(list(source = value))
  if (!is.null(value$value)) return(value$value)
  value
}

.submission_literal <- function(expression) {
  literal <- .submission_expression(expression)$literal
  if (is.list(literal)) literal <- literal$value
  literal
}

.submission_is_integer <- function(value) {
  is.integer(value) && length(value) == 1L && !is.na(value)
}

.submission_origin_admitted <- function(expression, intermediates) {
  expression <- .submission_expression(expression)
  if (is.null(expression)) return(NULL)
  operation <- names(expression)[1L]
  if (identical(operation, "literal")) return(c("Assigned", "Protocol", "Other"))
  source <- expression$source
  if (is.list(source)) source <- source$variable
  intermediate <- is.character(source) && sub("\\..*$", "", source) %in% intermediates
  if (operation %in% c("source", "odm") && !intermediate) {
    return(c("Collected", "Assigned", "Protocol", "Predecessor", "Other", "Not Available"))
  }
  c("Derived", "Assigned", "Other")
}

#' Validate submission metadata on a resolved specification
#'
#' Applies the family-independent rules REQ-0855 through REQ-0925, including
#' metadata lengths, required origins, methods, and graph refutations. It
#' returns diagnostics without changing the specification.
#' @param specification A resolved specification represented as an R list.
#' @return A list of portable validation diagnostics, empty on success.
#' @export
validate_submission_metadata <- function(specification) {
  diagnostics <- list()
  fail <- function(condition, path, requirement, context = list()) {
    diagnostics[[length(diagnostics) + 1L]] <<-
      .submission_diagnostic(condition, path, requirement, context)
  }
  governed_dataset <- c("label", "class", "subclass", "structure", "repeating",
                       "reference_data", "domain", "comment")
  governed_column <- c("core", "mandatory", "role", "data_type", "length",
                      "significant_digits", "display_format", "codelist",
                      "inventory_vocabulary", "origin", "method", "comment")
  defaults <- c(str = "text", int = "integer", float = "float", date = "date",
                datetime = "datetime")
  string_types <- c("text", "date", "datetime", "time", "partialDate",
                    "partialTime", "partialDatetime", "incompleteDate",
                    "incompleteTime", "incompleteDatetime", "durationDatetime",
                    "intervalDatetime", "URI")
  for (key in intersect(names(specification$metadata), governed_dataset)) {
    fail("reserved_metadata_key", paste0("metadata.", key), "REQ-0910",
         list(key = key, level = "root"))
  }
  root <- specification$submission
  if (isTRUE(root$reference_data) && isTRUE(root$repeating)) {
    fail("reference_data_repeating_conflict", "submission", "REQ-0925",
         list(reference_data = TRUE, repeating = TRUE))
  }
  intermediates <- vapply(specification$intermediates, function(x) x$id, "")
  column_names <- vapply(specification$columns, function(x) x$name, "")
  for (column in specification$columns) {
    name <- column$name
    base <- paste0("columns.", name)
    for (key in intersect(names(column$metadata), governed_column)) {
      fail("reserved_metadata_key", paste0(base, ".metadata.", key), "REQ-0910",
           list(key = key, level = "column", column = name))
    }
    meta <- column$submission
    if (is.null(meta)) next
    path <- paste0(base, ".submission")
    context <- list(column = name)
    for (key in setdiff(names(meta), governed_column)) {
      fail("unknown_field", paste0(path, ".", key), "REQ-0907", list(field = key))
    }
    if (!(name %in% unlist(specification$output$columns))) {
      fail("submission_not_output_column", path, "REQ-0909", context)
    }
    datatype <- meta$data_type
    if (is.null(datatype)) datatype <- defaults[[column$type]]
    admitted <- if (column$type == "str") string_types else defaults[[column$type]]
    if (!(datatype %in% admitted)) {
      fail("submission_data_type_not_admitted", path, "REQ-0914",
           c(context, list(declared_type = column$type, data_type = datatype)))
    }
    whole <- function(value, minimum) {
      .submission_is_integer(value) && value >= minimum
    }
    if (!is.null(meta$length) && !whole(meta$length, 1)) {
      fail("submission_length_invalid", paste0(path, ".length"), "REQ-0911",
           c(context, list(length = meta$length)))
    }
    if (!is.null(meta$significant_digits) && !whole(meta$significant_digits, 0)) {
      fail("submission_length_invalid", paste0(path, ".significant_digits"),
           "REQ-0911", c(context, list(significant_digits = meta$significant_digits)))
    }
    checks <- .submission_checks(column)
    operations <- vapply(checks, function(x) names(x)[1L], "")
    maxima <- unlist(lapply(checks, function(x) x$max_length$max))
    if (!is.null(meta$length) && length(maxima) && any(maxima != meta$length)) {
      fail("declared_length_conflict", path, "REQ-0913",
           c(context, list(length = meta$length, max_length = maxima[1L])))
    }
    if (!is.null(root)) {
      if (datatype %in% c("text", "integer", "float") &&
          is.null(meta$length) && !length(maxima)) {
        fail("submission_length_missing", path, "REQ-0912",
             c(context, list(data_type = datatype)))
      }
      if (!(datatype %in% c("text", "integer", "float")) &&
          !is.null(meta$length) && !length(maxima)) {
        fail("submission_length_not_applicable", path, "REQ-0912", context)
      }
      if (datatype == "float" && is.null(meta$significant_digits)) {
        fail("submission_length_missing", path, "REQ-0912",
             c(context, list(field = "significant_digits")))
      }
      if (datatype != "float" && !is.null(meta$significant_digits)) {
        fail("submission_length_not_applicable", path, "REQ-0912",
             c(context, list(field = "significant_digits")))
      }
      if (is.null(meta$core)) {
        fail("core_mandatory_conflict", path, "REQ-0915",
             c(context, list(reason = "core_missing")))
      }
    }
    if (isTRUE(meta$mandatory) && !(name %in% unlist(specification$keys)) &&
        !("not_missing" %in% operations)) {
      fail("mandatory_not_enforced", path, "REQ-0916", context)
    }
    origin <- meta$origin
    if (is.null(origin)) {
      fail("origin_missing", path, "REQ-0918", context)
      next
    }
    if (origin$type %in% c("Predecessor", "Other", "Not Available") &&
        (is.null(origin$description) || !nzchar(origin$description))) {
      fail("origin_description_missing", path, "REQ-0920",
           c(context, list(origin_type = origin$type)))
    }
    if (origin$type == "Derived" && is.null(meta$method)) {
      fail("method_missing", path, "REQ-0923", context)
    }
    expressions <- list()
    for (row in specification$rows) {
      if (!is.null(row$derivations[[name]])) {
        expressions[[length(expressions) + 1L]] <- row$derivations[[name]]
      }
    }
    if (!length(expressions) && !is.null(column$derivation)) expressions <- list(column$derivation)
    if (length(expressions)) {
      admitted_origins <- Reduce(intersect, lapply(expressions,
        .submission_origin_admitted, intermediates = intermediates))
      if (!(origin$type %in% admitted_origins)) {
        fail("origin_contradicts_derivation", path, "REQ-0922",
             c(context, list(origin_type = origin$type, admitted = sort(admitted_origins))))
      }
    }
  }
  seen_values <- list()
  for (row in specification$rows) {
    discriminator <- paste0(specification$domain, "TESTCD")
    code <- .submission_literal(row$derivations[[discriminator]])
    if (length(row$submission) && !(discriminator %in% column_names)) {
      fail("value_metadata_requires_testcd", paste0("rows.", row$id, ".submission"), "REQ-1163")
      next
    }
    if (length(row$submission) && (!is.character(code) || length(code) != 1L)) {
      fail("value_metadata_requires_literal_testcd", paste0("rows.", row$id, ".submission"), "REQ-1164")
      next
    }
    for (name in names(row$submission)) {
      path <- paste0("rows.", row$id, ".submission.", name)
      meta <- row$submission[[name]]
      if (!(name %in% column_names)) {
        fail("value_metadata_unknown_column", path, "REQ-1165", list(column = name))
        next
      }
      if (!(name %in% unlist(specification$output$columns))) {
        fail("value_metadata_not_output_column", path, "REQ-1166", list(column = name))
      }
      origin <- meta$origin
      if (is.null(origin)) {
        fail("origin_missing", path, "REQ-0918", list(column = name, row = row$id))
        next
      }
      if (origin$type %in% c("Predecessor", "Other", "Not Available") &&
          (is.null(origin$description) || !nzchar(origin$description))) {
        fail("origin_description_missing", path, "REQ-0920", list(column = name, row = row$id))
      }
      column <- specification$columns[[match(name, column_names)]]
      if (is.null(row$derivations[[name]]) && is.null(column$derivation)) {
        fail("value_metadata_unknown_column", path, "REQ-1165", list(column = name))
      }
      if (is.character(code) && length(code) == 1L) {
        identity <- paste(name, code, sep = "\r")
        if (!is.null(seen_values[[identity]]) && !.submission_equal_declarations(seen_values[[identity]], meta)) {
          fail("conflicting_value_metadata", path, "REQ-1167", list(column = name, testcd = code))
        }
        seen_values[[identity]] <- meta
      }
      if (origin$type == "Derived" && is.null(meta$method) &&
          is.null(column$submission$method)) {
        fail("method_missing", path, "REQ-0923", list(column = name, row = row$id))
      }
      expression <- row$derivations[[name]]
      if (is.null(expression)) expression <- column$derivation
      admitted <- .submission_origin_admitted(expression, intermediates)
      if (!is.null(admitted) && !(origin$type %in% admitted)) {
        fail("refuted_value_origin", path, "REQ-1168", list(column = name, row = row$id))
      }
    }
  }
  diagnostics
}

.submission_binding <- function(column, identifier, codelists, path) {
  code <- codelists[[identifier]]
  context <- list(column = column$name, codelist = identifier)
  if (is.null(code)) return(list(.submission_diagnostic(
    "unknown_codelist", path, "REQ-0953", context)))
  diagnostics <- list()
  datatype <- code$data_type
  if (is.null(datatype)) datatype <- "text"
  expected <- c(text = "str", integer = "int", float = "float")[[datatype]]
  if (column$type != expected) diagnostics <- list(.submission_diagnostic(
    "codelist_type_mismatch", path, "REQ-0954",
    c(context, list(codelist_data_type = datatype, column_type = column$type))))
  for (check in .submission_checks(column)) {
    if (is.null(check$allowed_values) || is.null(code$items) || isTRUE(code$extensible)) next
    listed <- unlist(check$allowed_values$values)
    bound <- unlist(lapply(code$items, function(x) x$value))
    if (!setequal(listed, bound)) diagnostics[[length(diagnostics) + 1L]] <-
      .submission_diagnostic("codelist_values_conflict", path, "REQ-0955", context)
  }
  diagnostics
}

#' Validate a study document's controlled terminology
#'
#' Checks codelist shape, duplicate identities and values, standard references,
#' column and value-level bindings, type agreement, and unused codelists.
#' @param document A study document represented as an R list.
#' @param specifications A named list of resolved specifications, keyed by dataset id.
#' @return A list of portable validation diagnostics, empty on success.
#' @export
validate_study_terminology <- function(document, specifications) {
  diagnostics <- list()
  fail <- function(condition, path, requirement, context = list()) {
    diagnostics[[length(diagnostics) + 1L]] <<-
      .submission_diagnostic(condition, path, requirement, context)
  }
  identifiers <- character()
  code_names <- character()
  codelists <- list()
  standards <- setNames(document$standards, vapply(document$standards, function(x) x$id, ""))
  for (code in document$codelists) {
    id <- code$id
    path <- paste0("codelists.", id)
    if (id %in% identifiers) fail("duplicate_define_identifier", path, "REQ-0948", list(codelist = id, field = "id"))
    if (code$name %in% code_names) fail("duplicate_define_identifier", path, "REQ-0948", list(codelist = id, field = "name", name = code$name))
    identifiers <- c(identifiers, id)
    code_names <- c(code_names, code$name)
    codelists[[id]] <- code
    if (!is.null(code$standard) && !identical(standards[[code$standard]]$type, "CT")) {
      fail("codelist_shape_invalid", paste0(path, ".standard"), "REQ-0958", list(codelist = id, standard = code$standard))
    }
    if (is.null(code$items) == is.null(code$external)) {
      fail("codelist_shape_invalid", path, "REQ-0947", list(codelist = id))
      next
    }
    datatype <- code$data_type
    if (is.null(datatype)) datatype <- "text"
    values <- list()
    decode <- logical()
    rank <- logical()
    for (i in seq_along(code$items)) {
      item <- code$items[[i]]
      item_path <- paste0(path, ".items[", i - 1L, "]")
      value <- item$value
      valid <- if (datatype == "text") is.character(value) else {
        is.numeric(value) && length(value) == 1L && is.finite(value) &&
          (datatype == "float" || .submission_is_integer(value))
      }
      if (!valid) fail("codelist_shape_invalid", item_path, "REQ-0950", list(codelist = id, value = value))
      equal <- vapply(values, function(prior) {
        if (is.numeric(prior) && is.numeric(value)) return(
          length(prior) == 1L && length(value) == 1L &&
            is.finite(prior) && is.finite(value) && prior == value)
        identical(prior, value)
      }, logical(1))
      if (any(equal)) fail("codelist_duplicate_value", item_path, "REQ-0949", list(codelist = id, value = value))
      values[[length(values) + 1L]] <- value
      decode <- c(decode, !is.null(item$decode))
      rank <- c(rank, !is.null(item$rank))
      if (isTRUE(item$extended) && (!isTRUE(code$extensible) || is.null(code$standard))) {
        fail("codelist_extension_not_admitted", item_path, "REQ-0952", list(codelist = id))
      }
    }
    for (field in c("decode", "rank")) {
      present <- if (field == "decode") decode else rank
      if (any(present) && !all(present)) fail("codelist_partial_item_field", path, "REQ-0951", list(codelist = id, field = field))
    }
  }
  used <- character()
  for (spec in specifications) {
    columns <- setNames(spec$columns, vapply(spec$columns, function(x) x$name, ""))
    for (column in spec$columns) {
      identifier <- column$submission$codelist
      if (!is.null(identifier)) {
        used <- c(used, identifier)
        diagnostics <- c(diagnostics, .submission_binding(column, identifier, codelists, paste0("columns.", column$name, ".submission")))
      }
    }
    for (row in spec$rows) {
      for (name in names(row$submission)) {
        identifier <- row$submission[[name]]$codelist
        if (is.null(identifier) || is.null(columns[[name]])) next
        used <- c(used, identifier)
        diagnostics <- c(diagnostics, .submission_binding(columns[[name]], identifier, codelists, paste0("rows.", row$id, ".submission.", name)))
      }
    }
  }
  for (identifier in setdiff(identifiers, used)) fail("unreferenced_codelist", paste0("codelists.", identifier), "REQ-0956", list(codelist = identifier))
  diagnostics
}

.submission_stop <- function(diagnostics) {
  if (!length(diagnostics)) return(invisible(NULL))
  stop(structure(list(message = paste(vapply(diagnostics, function(x) x$condition, ""), collapse = ", "), call = NULL, diagnostics = diagnostics),
                 class = c("yamaa_submission_error", "error", "condition")))
}

#' Load a study document and its resolved dataset specifications
#'
#' Uses YAML 1.2 and validates column metadata and terminology composition.
#' An engine can supply its specification loader to resolve inheritance before
#' these checks; the default reads specifications without parents.
#' @param define_path Path to the study document.
#' @param specification_loader Function taking a specification path and returning
#'   a resolved specification list. Default reads a standalone YAML 1.2 file.
#' @param schema_root Directory containing the canonical submission YAML schemas.
#' @return The document, specifications keyed by id, and written document path.
#' @export
load_study_document <- function(define_path, specification_loader = NULL,
                                schema_root = NULL) {
  path <- normalizePath(define_path, mustWork = TRUE)
  document <- yaml12::read_yaml(path, simplify = FALSE)
  schema <- .submission_schema(schema_root)
  .submission_stop(.submission_shape(document, "define_class", schema, "define"))
  if (!identical(document$schema_version, schema$version)) {
    .submission_stop(list(.submission_diagnostic("schema_version_mismatch", "schema_version", "REQ-1061")))
  }
  document <- .submission_normalize(document, "define_class", schema)
  if (is.null(specification_loader)) specification_loader <- function(spec_path) {
    specification <- yaml12::read_yaml(spec_path, simplify = FALSE)
    if (length(specification$parents)) {
      stop("inherited specifications require the engine's resolving specification_loader")
    }
    specification
  }
  specifications <- list()
  for (entry in document$datasets) {
    specification <- specification_loader(file.path(dirname(path), entry$spec))
    .submission_stop(.submission_spec_shape(specification, schema))
    if (!is.null(specification$submission)) specification$submission <-
      .submission_normalize(specification$submission, "submission_dataset_class", schema)
    for (i in seq_along(specification$columns)) {
      column <- specification$columns[[i]]
      if (!is.null(column$submission)) specification$columns[[i]]$submission <-
        .submission_normalize(column$submission, "submission_column_class", schema)
    }
    for (i in seq_along(specification$rows)) {
      for (name in names(specification$rows[[i]]$submission)) {
        specification$rows[[i]]$submission[[name]] <- .submission_normalize(
          specification$rows[[i]]$submission[[name]], "submission_column_class", schema)
      }
    }
    .submission_stop(validate_submission_metadata(specification))
    specifications[[entry$id]] <- specification
  }
  .submission_stop(validate_study_terminology(document, specifications))
  list(document = document, specifications = specifications, written_path = path)
}

#' Check a completed column against its study's terminology
#'
#' Run at the column verification boundary, after conversion and handlers.
#' Missing values pass; extensible and external codelists add no constraint.
#' @param data A completed data.frame.
#' @param column One resolved column declaration.
#' @param specification The resolved specification containing the column.
#' @param document The study document supplying the bound codelists.
#' @return Portable verification diagnostics with column, row keys, and values.
#' @export
check_submission_column <- function(data, column, specification, document) {
  codelists <- setNames(document$codelists, vapply(document$codelists, function(x) x$id, ""))
  default <- column$submission$codelist
  bindings <- rep(if (is.null(default)) "" else default, nrow(data))
  discriminator <- paste0(specification$domain, "TESTCD")
  for (row in specification$rows) {
    override <- row$submission[[column$name]]$codelist
    code <- .submission_literal(row$derivations[[discriminator]])
    if (!is.null(override) && is.character(code)) {
      selected <- !is.na(data[[discriminator]]) & data[[discriminator]] == code
      bindings[selected] <- override
    }
  }
  values <- data[[column$name]]
  diagnostics <- list()
  report <- function(bad, path, requirement, identifier = NULL) {
    if (!length(bad)) return(NULL)
    keys <- lapply(bad, function(i) as.list(data[i, unlist(specification$keys), drop = FALSE]))
    context <- list(column = column$name, failure_count = length(bad), keys = head(keys, 5L), values = as.list(head(values[bad], 5L)))
    if (!is.null(identifier)) context$codelist <- identifier
    .submission_diagnostic("allowed_values_failed", path, requirement, context, "verification")
  }
  for (identifier in unique(bindings[bindings != ""])) {
    code <- codelists[[identifier]]
    if (is.null(code)) stop("unknown bound codelist: ", identifier)
    if (isTRUE(code$extensible) || is.null(code$items)) next
    admitted <- unlist(lapply(code$items, function(x) x$value))
    bad <- which(bindings == identifier & !is.na(values) & !(values %in% admitted))
    failure <- report(bad, paste0("columns.", column$name, ".submission.codelist"), "REQ-0957", identifier)
    if (!is.null(failure)) diagnostics[[length(diagnostics) + 1L]] <- failure
  }
  if (isTRUE(column$submission$inventory_vocabulary)) {
    admitted <- vapply(document$datasets, function(x) x$id, "")
    failure <- report(which(!is.na(values) & !(values %in% admitted)), paste0("columns.", column$name, ".submission.inventory_vocabulary"), "REQ-1156")
    if (!is.null(failure)) diagnostics[[length(diagnostics) + 1L]] <- failure
  }
  diagnostics
}
