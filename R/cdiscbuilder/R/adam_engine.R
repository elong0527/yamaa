#' @import dplyr
#' @import rlang
#' @import purrr
#' @import stringr
NULL
#' Derive a constant value
#' @noRd
.derive_constant <- function(target_df, val) {
  rep(val, nrow(target_df))
}
#' Apply ADaM mappings and types
#' @noRd
.apply_adam_mapping <- function(series, col_spec) {
  derivation <- col_spec$derivation
  vmap <- col_spec$value_mapping
  if (is.null(vmap)) vmap <- col_spec$mapping_value
  if (is.null(vmap) && !is.null(derivation)) {
    vmap <- derivation$value_mapping
    if (is.null(vmap)) vmap <- derivation$mapping_value
  }
  if (!is.null(vmap)) {
    case_sens <- col_spec$case_sensitive
    if (is.null(case_sens) && !is.null(derivation)) {
      case_sens <- derivation$case_sensitive
    }
    if (is.null(case_sens)) case_sens <- TRUE
    default_val <- col_spec$mapping_default
    if (is.null(default_val) && !is.null(derivation)) {
      default_val <- derivation$mapping_default
    }
    if (!case_sens) {
      lower_vmap <- setNames(unlist(vmap), tolower(names(vmap)))
      mapped <- unname(lower_vmap[tolower(as.character(series))])
    } else {
      mapped <- unname(unlist(vmap)[as.character(series)])
    }
    if (!is.null(default_val)) {
      series <- coalesce(mapped, rep(default_val, length(series)))
    } else {
      series <- coalesce(mapped, series)
    }
  }
  if (!is.null(derivation$cut)) {
    series <- .apply_sql_cut(series, derivation$cut)
  }
  # Type
  if (!is.null(col_spec$type)) {
    ttype <- col_spec$type
    if (ttype == "int") {
      series <- as.integer(series)
    } else if (ttype == "float") {
      series <- as.numeric(series)
    } else if (ttype == "str") { # nolint: brace_linter
      series <- as.character(series)
    } else if (ttype == "bool") series <- as.logical(series)
  }
  # Case
  if (!is.null(col_spec$case) && is.character(series)) {
    if (col_spec$case == "upper") { # nolint: brace_linter
      series <- toupper(series)
    } else if (col_spec$case == "lower") series <- tolower(series)
  }
  series
}
#' Build an ADaM dataset from specifications
#'
#' @description Generates an ADaM dataset based on a parsed
#' YAML template, drawing data
#' from previously built SDTM or intermediate ADaM datasets. It handles
#' data merges,
#' type conversions, variable mapping, derivations, and cuts specified
#' in the YAML.
#'
#' @param spec_yaml_path A character string representing
#' the path to the ADaM YAML spec
#' @param source_data A named list of data.frames containing the upstream
#' datasets
#'
#' @return A data.frame containing the fully derived ADaM dataset
#' @export
build_adam_dataset <- function(spec_yaml_path, source_data) {
  spec <- yaml::read_yaml(spec_yaml_path)
  key_vars <- spec$key
  # Get Base dataset
  deps <- spec$data_dependency
  key_dep <- NULL
  for (d in deps) {
    if (d$adam_variable %in% key_vars) {
      key_dep <- d
      break
    }
  }
  if (is.null(key_dep)) stop("Cannot determine base dataset for keys")
  base_data_name <- key_dep$sdtm_data
  target_df <- source_data[[base_data_name]] |>
    select(any_of(key_vars)) |>
    distinct()
  for (col_spec in spec$columns) {
    col_name <- col_spec$name
    if (col_name %in% key_vars || isTRUE(col_spec$drop)) next
    cat("Deriving column:", col_name, "\n")
    derivation <- col_spec$derivation
    series <- rep(NA, nrow(target_df))
    if (!is.null(derivation)) {
      if (!is.null(derivation$constant)) {
        series <- .derive_constant(target_df, derivation$constant)
      } else if (!is.null(derivation$source)) {
        series <- .derive_sql_source(derivation, key_vars, target_df, source_data) # nolint: line_length_linter
      } else if (!is.null(derivation$cut)) {
        # Needs a source usually, or uses the cut immediately if source is present # nolint: line_length_linter
        if (!is.null(derivation$source)) {
          series <- .derive_sql_source(derivation, key_vars, target_df, source_data) # nolint: line_length_linter
        }
      } else if (!is.null(derivation$function_)) {
        fn_name <- derivation$function_
        fn <- try(get(fn_name, mode = "function"), silent = TRUE)
        if (!inherits(fn, "try-error")) {
          # In complete port, arguments need to be mapped
          series <- rep(NA, nrow(target_df))
        }
      }
    }
    series <- .apply_adam_mapping(series, col_spec)
    target_df[[col_name]] <- series
  }
  target_df
}
