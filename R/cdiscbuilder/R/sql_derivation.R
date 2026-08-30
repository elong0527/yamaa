#' @import dplyr
#' @import rlang
#' @import stringr
#' @import sqldf
NULL
#' Execute SQL query and return result as Series
#' @noRd
.execute_sql <- function(sql, key_vars, target_df, source_data) {
  # Check for special CLOSEST handling
  if (str_starts(sql, "CLOSEST:")) {
    return(.execute_closest(sql, key_vars, target_df, source_data))
  }
  merged_df <- target_df
  # Add source data if needed
  for (dataset_name in names(source_data)) {
    if (str_detect(sql, dataset_name) ||
      str_detect(sql, paste0("\"", dataset_name, "\\."))) { # nolint: indentation_linter
      df <- source_data[[dataset_name]]
      available_keys <- intersect(key_vars, names(df))
      if (length(available_keys) > 0 && !(dataset_name %in% names(merged_df))) {
        # Prefix the merged columns to avoid name clashes
        cols_to_rename <- setdiff(names(df), available_keys)
        new_names <- paste0(dataset_name, ".", cols_to_rename) # nolint: object_usage_linter
        df <- df |> rename_with(~new_names, all_of(cols_to_rename))
        merged_df <- merged_df |> left_join(df, by = available_keys)
      }
    }
  }
  # Ensure dataset is named 'merged' for the SQL query
  merged <- merged_df # nolint
  # Execute using sqldf
  # We replace `DM.COLUMN` with `[DM.COLUMN]` to handle dots in SQLite
  sql_quoted <- str_replace_all(sql, "(\\w+)\\.(\\w+)", "[\\1.\\2]")
  result_df <- tryCatch(
    {
      sqldf::sqldf(sql_quoted)
    },
    error = function(e) {
      warning("SQL execution failed: ", conditionMessage(e))
      NULL
    }
  )
  if (is.null(result_df) || nrow(result_df) == 0) {
    return(rep(NA, nrow(target_df)))
  }
  if (length(key_vars) > 0) {
    # Deduplicate on key_vars
    result_df <- result_df |>
      distinct(across(all_of(key_vars)), .keep_all = TRUE)
    final_df <- target_df |>
      select(all_of(key_vars)) |>
      left_join(result_df, by = key_vars)
    final_df$result
  } else if (nrow(result_df) == nrow(target_df)) {
    result_df$result
  } else {
    rep(NA, nrow(target_df))
  }
}
#' Execute 'closest' aggregation using native R operations
#' @noRd
.execute_closest <- function(sql_spec, key_vars, target_df, source_data) {
  parts <- str_split(sql_spec, ":", n = 4)[[1]]
  source_col <- parts[2]
  target_col <- parts[3]
  filter_expr <- if (length(parts) > 3 && parts[4] != "") parts[4] else NULL
  dataset_name <- str_split(source_col, "\\.")[[1]][1]
  merged_df <- target_df |> mutate(`_row_id` = row_number())
  for (ds_name in names(source_data)) {
    if (ds_name == dataset_name || str_detect(target_col, ds_name)) {
      df <- source_data[[ds_name]]
      available_keys <- intersect(key_vars, names(df))
      if (length(available_keys) > 0) {
        cols_to_rename <- setdiff(names(df), available_keys)
        new_names <- paste0(ds_name, ".", cols_to_rename) # nolint: object_usage_linter
        df <- df |> rename_with(~new_names, all_of(cols_to_rename))
        merged_df <- merged_df |> left_join(df, by = available_keys)
      }
    }
  }
  filtered_df <- merged_df
  if (!is.null(filter_expr)) {
    # Attempt basic filter conversion
    f_expr <- str_replace_all(filter_expr, "\\b=\\b", "==")
    f_expr <- str_replace_all(f_expr, "\\band\\b", "&")
    f_expr <- str_replace_all(f_expr, "\\bor\\b", "|")
    filtered_df <- tryCatch(
      {
        filtered_df |> filter(eval(parse_expr(f_expr)))
      },
      error = function(e) {
        warning("Filter failed: ", conditionMessage(e))
        merged_df
      }
    )
  }
  if (dataset_name == "VS") {
    date_col <- paste0(dataset_name, ".VSDTC")
  } else if (dataset_name == "FA") {
    date_col <- paste0(dataset_name, ".FADTC")
  } else {
    date_col <- paste0(dataset_name, ".DTC")
  }
  result_list <- lapply(seq_len(nrow(target_df)), function(row_id) {
    subject_data <- filtered_df |> filter(.data$`_row_id` == row_id)
    if (nrow(subject_data) > 0 && source_col %in% names(subject_data)) {
      if (target_col %in% names(subject_data) &&
        date_col %in% names(subject_data)) { # nolint: indentation_linter
        target_date <- subject_data[[target_col]][1]
        if (is.na(target_date)) {
          return(subject_data[[source_col]][1])
        }
        # Calculate date difference
        t_date <- as.POSIXct(
          target_date,
          format = "%Y-%m-%dT%H:%M:%S", optional = TRUE
        )
        if (is.na(t_date)) {
          t_date <- as.POSIXct(
            target_date,
            format = "%Y-%m-%d %H:%M:%S", optional = TRUE
          )
        }
        if (is.na(t_date)) {
          t_date <- as.POSIXct(
            target_date,
            format = "%Y-%m-%d", optional = TRUE
          )
        }
        d_dates <- as.POSIXct(
          subject_data[[date_col]],
          format = "%Y-%m-%dT%H:%M:%S", optional = TRUE
        )
        if (all(is.na(d_dates))) {
          d_dates <- as.POSIXct(
            subject_data[[date_col]],
            format = "%Y-%m-%d %H:%M:%S",
            optional = TRUE
          )
        }
        if (all(is.na(d_dates))) {
          d_dates <- as.POSIXct(
            subject_data[[date_col]],
            format = "%Y-%m-%d", optional = TRUE
          )
        }
        diffs <- abs(as.numeric(difftime(d_dates, t_date, units = "secs")))
        min_idx <- which.min(diffs)
        if (length(min_idx) > 0) {
          return(subject_data[[source_col]][min_idx]) # nolint: return_linter
        } else {
          return(NA) # nolint: return_linter
        }
      } else {
        return(subject_data[[source_col]][1]) # nolint: return_linter
      }
    } else {
      return(NA) # nolint: return_linter
    }
  })
  unname(unlist(result_list))
}
#' Apply cut categorizations using SQL CASE WHEN
#' @noRd
.apply_sql_cut <- function(series, cuts) {
  if (is.null(series) || length(series) == 0) {
    return(series)
  }
  temp_series <- suppressWarnings(as.numeric(series))
  if (all(is.na(temp_series) & !is.na(series))) {
    temp_series <- series
  }
  frame <- data.frame(`_val` = temp_series, check.names = FALSE) # nolint
  source_name <- "[_val]"
  case_parts <- c()
  for (cond_name in names(cuts)) {
    label <- cuts[[cond_name]]
    condition <- tolower(trimws(cond_name))
    sql_condition <- str_replace_all(condition, "and", "AND")
    sql_condition <- str_replace_all(sql_condition, "or", "OR")
    sql_condition <- str_replace_all(sql_condition, ">=", "___GTE___")
    sql_condition <- str_replace_all(sql_condition, "<=", "___LTE___")
    sql_condition <- str_replace_all(sql_condition, ">", "___GT___")
    sql_condition <- str_replace_all(sql_condition, "<", "___LT___")
    sql_condition <- str_replace_all(sql_condition, "==", "___EQ___")
    sql_condition <- str_replace_all(sql_condition, "=", "___EQ___")
    sql_condition <- str_replace_all(
      sql_condition, "___GTE___",
      paste(">=", source_name, "AND", source_name, ">=")
    )
    sql_condition <- str_replace_all(
      sql_condition, "___LTE___",
      paste("<=", source_name, "AND", source_name, "<=")
    )
    sql_condition <- str_replace_all(
      sql_condition, "___GT___",
      paste(">", source_name, "AND", source_name, ">")
    )
    sql_condition <- str_replace_all(
      sql_condition, "___LT___",
      paste("<", source_name, "AND", source_name, "<")
    )
    sql_condition <- str_replace_all(
      sql_condition, "___EQ___",
      paste("=", source_name, "AND", source_name, "=")
    )
    if (str_detect(condition, " and ")) {
      parts <- str_split(condition, " and ")[[1]]
      lower_part <- trimws(parts[1])
      upper_part <- trimws(parts[2])
      lower_val <- str_replace_all(lower_part, "[>=<]+", "")
      upper_val <- str_replace_all(upper_part, "[>=<]+", "")
      lower_op <- if (str_starts(lower_part, ">=")) ">=" else ">"
      upper_op <- if (str_starts(upper_part, "<=")) "<=" else "<"
      case_parts <- c(
        case_parts,
        paste(
          "WHEN", source_name, lower_op, lower_val,
          "AND", source_name, upper_op, upper_val,
          "THEN", sprintf("'%s'", label)
        )
      )
    } else if (str_starts(condition, "<=")) {
      val <- str_trim(substring(condition, 3))
      case_parts <- c(
        case_parts,
        paste("WHEN", source_name, "<=", val, "THEN", sprintf("'%s'", label))
      )
    } else if (str_starts(condition, "<")) {
      val <- str_trim(substring(condition, 2))
      case_parts <- c(
        case_parts,
        paste("WHEN", source_name, "<", val, "THEN", sprintf("'%s'", label))
      )
    } else if (str_starts(condition, ">=")) {
      val <- str_trim(substring(condition, 3))
      case_parts <- c(
        case_parts,
        paste("WHEN", source_name, ">=", val, "THEN", sprintf("'%s'", label))
      )
    } else if (str_starts(condition, ">")) {
      val <- str_trim(substring(condition, 2))
      case_parts <- c(
        case_parts,
        paste("WHEN", source_name, ">", val, "THEN", sprintf("'%s'", label))
      )
    } else {
      case_parts <- c(
        case_parts,
        paste("WHEN", condition, "THEN", sprintf("'%s'", label))
      )
    }
  }
  case_expr <- paste("CASE", paste(case_parts, collapse = " "), "ELSE NULL END")
  sql_query <- paste("SELECT", case_expr, "as result FROM frame")
  result_df <- tryCatch(
    {
      sqldf::sqldf(sql_query)
    },
    error = function(e) {
      warning("Cut execution failed: ", conditionMessage(e))
      NULL
    }
  )
  if (!is.null(result_df)) {
    return(result_df$result)
  }
  rep(NA, length(series))
}
#' Build SQL for simple source derivation
#' @noRd
.build_source_sql <- function(source_col, filter_expr, mapping, key_vars) {
  if (!is.null(mapping)) {
    case_parts <- c()
    for (key in names(mapping)) {
      val <- mapping[[key]]
      if (key == "") {
        case_parts <- c(case_parts, paste("WHEN", source_col, "= '' THEN NULL"))
      } else if (is.null(val) || val == "Null") {
        case_parts <- c(
          case_parts,
          paste("WHEN", source_col, "=", sprintf("'%s'", key), "THEN NULL")
        )
      } else {
        case_parts <- c(
          case_parts,
          paste(
            "WHEN", source_col, "=", sprintf("'%s'", key),
            "THEN", sprintf("'%s'", val)
          )
        )
      }
    }
    select_expr <- paste(
      "CASE", paste(case_parts, collapse = " "), "ELSE NULL END as result"
    )
  } else {
    select_expr <- paste(source_col, "as result")
  }
  sql <- paste(
    "SELECT", paste(key_vars, collapse = ", "), ",", select_expr, "FROM merged"
  )
  if (!is.null(filter_expr)) {
    sql <- paste(sql, "WHERE", filter_expr)
  }
  sql
}
#' Build SQL for aggregation derivation
#' @noRd
.build_aggregation_sql <- function(
  source_col, agg_spec, filter_expr, key_vars
) {
  func <- agg_spec$function_
  if (is.null(func)) func <- "first"
  if (func == "first") {
    # SQLite does not have FIRST(), we can approximate with MIN
    agg_expr <- paste("MIN(", source_col, ") as result")
  } else if (func == "last") {
    agg_expr <- paste("MAX(", source_col, ") as result")
  } else if (func == "mean") {
    agg_expr <- paste("AVG(CAST(", source_col, "AS REAL)) as result")
  } else if (func == "sum") {
    agg_expr <- paste("SUM(CAST(", source_col, "AS REAL)) as result")
  } else if (func == "max") {
    agg_expr <- paste("MAX(", source_col, ") as result")
  } else if (func == "min") {
    agg_expr <- paste("MIN(", source_col, ") as result")
  } else if (func == "closest") {
    target <- agg_spec$target
    if (is.null(target)) stop("'closest' aggregation requires 'target' field")
    paste0(
      "CLOSEST:", source_col, ":", target, ":",
      if (!is.null(filter_expr)) filter_expr else ""
    )
  } else {
    stop("Unknown aggregation function: ", func)
  }
  sql <- paste(
    "SELECT", paste(key_vars, collapse = ", "), ",", agg_expr, "FROM merged"
  )
  if (!is.null(filter_expr)) {
    sql <- paste(sql, "WHERE", filter_expr)
  }
  sql <- paste(sql, "GROUP BY", paste(key_vars, collapse = ", "))
  sql
}
#' Derive from source with optional mapping, filter, and aggregation using SQL
#' @noRd
.derive_sql_source <- function(derivation, key_vars, target_df, source_data) {
  source_str <- derivation$source
  if (str_detect(source_str, "\\.")) {
    parts <- str_split(source_str, "\\.", n = 2)[[1]]
    dataset_name <- parts[1]
    col_name <- parts[2]
    source_col <- paste0(dataset_name, ".", col_name)
    if (!is.null(derivation$aggregation)) {
      sql_query <- .build_aggregation_sql(
        source_col, derivation$aggregation, derivation$filter, key_vars
      )
    } else {
      sql_query <- .build_source_sql(
        source_col, derivation$filter, derivation$mapping, key_vars
      )
    }
    series <- .execute_sql(sql_query, key_vars, target_df, source_data)
  } else {
    if (source_str %in% names(target_df)) {
      series <- target_df[[source_str]]
    } else {
      stop("Column ", source_str, " not found in target dataset")
    }
  }
  if (is.character(series)) {
    series <- trimws(series)
  }
  if (!str_detect(source_str, "\\.") && !is.null(derivation$mapping)) {
    series <- .apply_sql_mapping(series, derivation$mapping)
  }
  series
}
#' Temporary wrapper for standard value mapping
#' @noRd
.apply_sql_mapping <- function(series, mapping) {
  if (is.null(mapping)) {
    return(series)
  }
  mapped <- unname(unlist(mapping)[as.character(series)])
  coalesce(mapped, series)
}
