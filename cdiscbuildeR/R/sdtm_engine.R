#' @import dplyr
#' @import tidyr
#' @import purrr
#' @import stringr
NULL
#' Extract dependencies from domain configuration
#' @noRd
.extract_dependencies <- function(domain_name, sources) {
  deps <- character(0)
  if (is.list(sources) && !is.null(sources$type) && sources$type == "FINDINGS") { # nolint: line_length_linter
    # Scan definitions
    if (!is.null(sources$definitions)) {
      for (col_name in names(sources$definitions)) {
        col_cfg <- sources$definitions[[col_name]]
        if (is.list(col_cfg)) {
          src <- col_cfg$source
          if (is.character(src) && length(src) == 1 && str_detect(src, "\\.")) {
            ref_domain <- str_split(src, "\\.", n = 2)[[1]][1]
            if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
              deps <- c(deps, ref_domain)
            }
          }
          args <- col_cfg$args
          if (is.list(args) || is.character(args)) {
            for (arg in args) {
              if (is.character(arg) && length(arg) == 1 && str_detect(arg, "\\.")) { # nolint: line_length_linter
                ref_domain <- str_split(arg, "\\.", n = 2)[[1]][1]
                if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
                  deps <- c(deps, ref_domain)
                }
              }
            }
          }
        }
      }
    }
    # Scan columns
    if (!is.null(sources$columns)) {
      for (finding in sources$columns) {
        reserved <- c("name", "type", "formoid", "itemoid", "keys")
        mappings <- finding[setdiff(names(finding), reserved)]
        for (col_name in names(mappings)) {
          col_cfg <- mappings[[col_name]]
          if (is.list(col_cfg)) {
            src <- col_cfg$source
            if (is.character(src) && length(src) == 1 && str_detect(src, "\\.")) { # nolint: line_length_linter
              ref_domain <- str_split(src, "\\.", n = 2)[[1]][1]
              if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
                deps <- c(deps, ref_domain)
              }
            }
            args <- col_cfg$args
            if (is.list(args) || is.character(args)) {
              for (arg in args) {
                if (is.character(arg) && length(arg) == 1 && str_detect(arg, "\\.")) { # nolint: line_length_linter
                  ref_domain <- str_split(arg, "\\.", n = 2)[[1]][1]
                  if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
                    deps <- c(deps, ref_domain)
                  }
                }
              }
            }
          }
        }
      }
    }
  } else {
    if (!is.null(names(sources))) {
      sources <- list(sources)
    }
    for (source in sources) {
      columns <- source$columns
      if (!is.null(columns)) {
        for (col_name in names(columns)) {
          col_cfg <- columns[[col_name]]
          if (is.list(col_cfg)) {
            src <- col_cfg$source
            if (is.character(src) && length(src) == 1 && str_detect(src, "\\.")) { # nolint: line_length_linter
              ref_domain <- str_split(src, "\\.", n = 2)[[1]][1]
              if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
                deps <- c(deps, ref_domain)
              }
            }
            args <- col_cfg$args
            if (is.list(args) || is.character(args)) {
              for (arg in args) {
                if (is.character(arg) && length(arg) == 1 && str_detect(arg, "\\.")) { # nolint: line_length_linter
                  ref_domain <- str_split(arg, "\\.", n = 2)[[1]][1]
                  if (ref_domain != domain_name && str_detect(ref_domain, "^_?[A-Z_]+$")) { # nolint: line_length_linter
                    deps <- c(deps, ref_domain)
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  unique(deps)
}
#' Topological Sort of Domains
#' @export
topological_sort <- function(domains_config) {
  all_domains <- names(domains_config)
  graph <- list()
  for (domain in all_domains) {
    sources <- domains_config[[domain]]
    graph[[domain]] <- .extract_dependencies(domain, sources)
  }
  visited <- character(0)
  temp_mark <- character(0)
  order <- character(0)
  visit <- function(node) {
    if (node %in% temp_mark) {
      stop(paste("Circular dependency detected involving", node))
    }
    if (node %in% visited) return()
    temp_mark <<- c(temp_mark, node) # nolint
    deps <- graph[[node]]
    for (dep in deps) {
      if (dep %in% all_domains) {
        visit(dep)
      }
    }
    temp_mark <<- setdiff(temp_mark, node) # nolint
    visited <<- c(visited, node) # nolint
    order <<- c(order, node) # nolint
  }
  for (domain in all_domains) {
    visit(domain)
  }
  order
}
#' Apply mapping transformations to a dataset column
#'
#' @description Applies a variety of SDTM mapping operations
#' (e.g., direct variable mapping,
#' constant assignment,
#' logic branching, regex extraction, external function calls) based on
#' a column specification defined in a YAML template.
#'
#' @param pivoted A data.frame containing the pivoted source data
#' @param col_name A character string of the column being mapped
#' @param col_cfg A list specifying the transformation rules (from YAML)
#' @param built_domains A list of previously built SDTM domains
#' @param final_df A data.frame representing the target SDTM structure
#' being built
#'
#' @return A transformed vector conforming to the target SDTM column spec
.apply_column_mapping <- function(pivoted, col_name, col_cfg, built_domains, final_df) { # nolint: line_length_linter
  if (is.character(col_cfg)) {
    col_cfg <- list(source = col_cfg)
  }
  series <- rep(NA, nrow(pivoted))
  if (!is.null(col_cfg$literal)) {
    series <- rep(col_cfg$literal, nrow(pivoted))
  } else if (!is.null(col_cfg$function_)) {
    func_name <- col_cfg$function_
    args_list <- list()
    for (arg in col_cfg$args) {
      if (arg %in% names(final_df)) {
        args_list[[length(args_list) + 1]] <- final_df[[arg]]
      } else if (arg %in% names(pivoted)) {
        args_list[[length(args_list) + 1]] <- pivoted[[arg]]
      } else if (str_detect(arg, "\\.")) {
        parts <- str_split(arg, "\\.", n = 2)[[1]]
        ref_domain <- parts[1]
        ref_col <- parts[2]
        if (!is.null(built_domains[[ref_domain]]) && ref_col %in% names(built_domains[[ref_domain]])) { # nolint: line_length_linter
          args_list[[length(args_list) + 1]] <- built_domains[[ref_domain]][[ref_col]][match(final_df$USUBJID, built_domains[[ref_domain]]$USUBJID)] # nolint: line_length_linter
        } else {
          args_list[[length(args_list) + 1]] <- rep(NA, nrow(pivoted))
        }
      } else {
        args_list[[length(args_list) + 1]] <- rep(NA, nrow(pivoted))
      }
    }
    fn <- get(func_name, mode = "function")
    series <- do.call(fn, c(args_list, built_domains = list(built_domains)))
  } else if (!is.null(col_cfg$source)) {
    src <- col_cfg$source
    if (src %in% names(pivoted)) {
      series <- pivoted[[src]]
    } else if (src %in% names(final_df)) {
      series <- final_df[[src]]
    } else if (str_detect(src, "\\.")) {
      # Cross domain reference
      parts <- str_split(src, "\\.", n = 2)[[1]]
      ref_domain <- parts[1]
      ref_col <- parts[2]
      if (!is.null(built_domains[[ref_domain]]) && ref_col %in% names(built_domains[[ref_domain]])) { # nolint: line_length_linter
        ref_df <- built_domains[[ref_domain]]
        merge_keys <- if (!is.null(col_cfg$merge_on)) col_cfg$merge_on else "USUBJID" # nolint: line_length_linter
        # Simple left join
        valid_keys <- intersect(merge_keys, intersect(names(final_df), names(ref_df))) # nolint: line_length_linter
        if (length(valid_keys) > 0) {
          ref_subset <- ref_df |>
            select(all_of(c(valid_keys, ref_col))) |>
            distinct(across(all_of(valid_keys)), .keep_all = TRUE)
          merged <- final_df |>
            select(all_of(valid_keys)) |>
            left_join(ref_subset, by = valid_keys)
          series <- merged[[ref_col]]
        }
      }
    } else if (src %in% names(pivoted)) {
      series <- pivoted[[src]]
    } else if (src %in% names(final_df)) {
      series <- final_df[[src]]
    }
  }
  if (is.null(series)) series <- rep(NA, nrow(pivoted))
  # Fallback
  if (!is.null(col_cfg$fallback)) {
    fallback <- col_cfg$fallback
    if (fallback %in% names(pivoted)) {
      series <- coalesce(as.character(series), as.character(pivoted[[fallback]])) # nolint: line_length_linter
    } else if (fallback %in% names(final_df)) {
      series <- coalesce(as.character(series), as.character(final_df[[fallback]])) # nolint: line_length_linter
    }
  }
  # Value Mapping
  if (!is.null(col_cfg$value_mapping)) {
    vmap <- col_cfg$value_mapping
    mapped <- unname(unlist(vmap)[as.character(series)])
    if (is.null(col_cfg$case_sensitive) || col_cfg$case_sensitive == TRUE) {
      series <- coalesce(mapped, series)
    } else {
      # Case insensitive mapping
      lower_vmap <- setNames(unlist(vmap), tolower(names(vmap)))
      mapped_lower <- unname(lower_vmap[tolower(as.character(series))])
      series <- coalesce(mapped_lower, series)
    }
  }
  # Type Conversion
  if (!is.null(col_cfg$type)) {
    target_type <- col_cfg$type
    if (target_type == "int") {
      series <- as.integer(series)
    } else if (target_type == "float") {
      series <- as.numeric(series)
    } else if (target_type == "str") {
      series <- as.character(series)
    } else if (target_type == "bool") {
      series <- as.logical(series)
    }
  }
  series
}
#' Process a findings domain
#' @noRd
process_findings_domain <- function(domain_name, config, df_long, default_keys, built_domains = list()) { # nolint: line_length_linter
  domain_dfs <- list()
  findings <- config$columns
  for (finding in findings) {
    form_oid <- finding$formoid
    source_df <- df_long
    if (!is.null(form_oid)) {
      source_df <- source_df |> filter(.data$FormOID %in% form_oid)
    }
    if (nrow(source_df) == 0) next
    keys <- if (!is.null(finding$keys)) finding$keys else default_keys
    keys <- intersect(keys, names(source_df))
    pivoted <- source_df |>
      select(all_of(keys), "ItemOID", "Value") |>
      pivot_wider(
        names_from = "ItemOID", values_from = "Value", values_fn = first
      )
    final_df <- tibble::tibble(.rows = nrow(pivoted))
    for (k in keys) {
      if (k %in% names(pivoted)) final_df[[k]] <- pivoted[[k]]
    }
    reserved <- c("name", "type", "formoid", "itemoid", "keys")
    mappings <- finding[setdiff(names(finding), reserved)]
    if (length(mappings) > 0) {
      for (target_col in names(mappings)) {
        col_cfg <- mappings[[target_col]]
        final_df[[target_col]] <- .apply_column_mapping(
          pivoted, target_col, col_cfg, built_domains, final_df
        )
      }
    }
    target_cols <- c(names(mappings), keys)
    final_df <- final_df |> select(any_of(target_cols))
    domain_dfs[[length(domain_dfs) + 1]] <- final_df
  }
  if (length(domain_dfs) == 0) {
    return(NULL)
  }
  combined <- bind_rows(domain_dfs)
  if (!is.null(config$definitions)) {
    for (target_col in names(config$definitions)) {
      col_cfg <- config$definitions[[target_col]]
      if (is.list(col_cfg) && !is.null(col_cfg$group)) {
        grp_cols <- col_cfg$group
        sort_cols <- col_cfg$sort_by
        if (all(grp_cols %in% names(combined))) {
          temp <- combined |> mutate(..row_id = dplyr::row_number())
          if (!is.null(sort_cols) && all(sort_cols %in% names(combined))) {
            temp <- temp |> arrange(across(all_of(c(grp_cols, sort_cols))))
          } else {
            temp <- temp |> arrange(across(all_of(grp_cols)))
          }
          temp <- temp |>
            group_by(across(all_of(grp_cols))) |>
            mutate(..seq = dplyr::row_number()) |>
            ungroup() |>
            arrange(across(all_of("..row_id")))
          combined[[target_col]] <- temp$..seq
        }
      } else {
        combined[[target_col]] <- .apply_column_mapping(
          combined, target_col, col_cfg, built_domains, combined
        )
      }
    }
  }
  final_target_cols <- unique(
    c(
      names(config$definitions),
      unlist(lapply(findings, function(x) {
        set_names <- setdiff(
          names(x), c("name", "type", "formoid", "itemoid", "keys")
        )
        names(x[set_names])
      }))
    )
  )
  combined <- combined |> select(any_of(final_target_cols))
  combined
}
#' Process a single SDTM domain
#'
#' @description Generates an SDTM dataset based on a parsed
#' YAML template, pulling data from
#' long-format EDC clinical data, applying transformations,
#' and merging contextual lookup data.
#'
#' @param domain_name A character string of the domain name
#' @param sources A list representing the parsed YAML specification
#' for the domain
#' @param df_long A data.frame containing long-format clinical data (EDC)
#' @param default_keys A character vector of default keys
#' @param built_domains A list of previously built domains to use as context
#'
#' @return A data.frame containing the fully transformed
#' and populated SDTM domain
#' @export
process_domain <- function(
  domain_name, sources, df_long, default_keys, built_domains = list()
) {
  if (is.list(sources) && !is.null(sources$type) &&
    sources$type == "FINDINGS") { # nolint: indentation_linter
    return(
      process_findings_domain(
        domain_name, sources, df_long, default_keys, built_domains
      )
    )
  }
  if (!is.null(names(sources))) sources <- list(sources)
  domain_dfs <- list()
  for (settings in sources) {
    if (!is.null(settings$supp)) next # handled later
    # Filter
    form_oid <- settings$formoid
    source_df <- df_long
    if (!is.null(form_oid)) {
      source_df <- source_df |> filter(.data$FormOID %in% form_oid)
    }
    if (nrow(source_df) == 0) next
    # Pivot
    keys <- if (!is.null(settings$keys)) settings$keys else default_keys
    keys <- intersect(keys, names(source_df))
    pivoted <- source_df |>
      select(all_of(keys), ItemOID, Value) |> # nolint: object_usage_linter
      pivot_wider(names_from = ItemOID, values_from = Value, values_fn = first)
    # Map columns
    final_df <- tibble::tibble(.rows = nrow(pivoted))
    # Add keys to final_df first to allow cross-domain joins
    for (k in keys) {
      if (k %in% names(pivoted)) final_df[[k]] <- pivoted[[k]]
    }
    mappings <- settings$columns
    if (!is.null(mappings)) {
      for (target_col in names(mappings)) {
        col_cfg <- mappings[[target_col]]
        if (is.list(col_cfg) && !is.null(col_cfg$group)) {
          # Sequence generator (run later)
          final_df[[target_col]] <- NA
        } else {
          final_df[[target_col]] <- .apply_column_mapping(
            pivoted, target_col, col_cfg, built_domains, final_df
          )
        }
      }
      # Grouping Sequence Generation
      for (target_col in names(mappings)) {
        col_cfg <- mappings[[target_col]]
        if (is.list(col_cfg) && !is.null(col_cfg$group)) {
          grp_cols <- col_cfg$group
          sort_cols <- col_cfg$sort_by
          if (all(grp_cols %in% names(final_df))) {
            temp <- final_df |> mutate(..row_id = dplyr::row_number())
            if (!is.null(sort_cols) && all(sort_cols %in% names(final_df))) {
              temp <- temp |> arrange(across(all_of(c(grp_cols, sort_cols))))
            } else {
              temp <- temp |> arrange(across(all_of(grp_cols)))
            }
            temp <- temp |>
              group_by(across(all_of(grp_cols))) |>
              mutate(..seq = dplyr::row_number()) |>
              ungroup() |>
              arrange(across(all_of("..row_id")))
            final_df[[target_col]] <- temp$..seq
          }
        }
      }
    }
    # Keep only target mapped columns
    target_cols <- names(mappings)
    final_df <- final_df |> select(any_of(target_cols))
    domain_dfs[[length(domain_dfs) + 1]] <- final_df
  }
  if (length(domain_dfs) == 0) {
    return(NULL)
  }
  combined <- bind_rows(domain_dfs)
  combined
}
#' Build all SDTM datasets from specifications
#'
#' @description Orchestrates the entire SDTM build process
#' by loading all YAML specifications
#' in a directory, determining their topological dependencies, and processing
#' them in the
#' correct order using the long-format EDC data.
#'
#' @param config_dir The directory path containing the SDTM YAML specification files # nolint: line_length_linter
#' @param input_csv
#' The path to the input CSV containing long-format clinical data
#' @param output_dir The directory path where output CSV files will be saved
#'
#' @return A named list of data.frames representing the generated SDTM datasets
#' @export
create_sdtm_datasets <- function(config_dir, input_csv, output_dir) {
  # Load Config
  schema_files <- list.files(
    config_dir,
    pattern = "\\.yaml$", full.names = TRUE
  )
  config <- list(domains = list())
  for (f in schema_files) {
    parsed <- yaml::read_yaml(f)
    for (d in names(parsed)) {
      config$domains[[d]] <- parsed[[d]]
    }
  }
  df_long <- read.csv(input_csv, stringsAsFactors = FALSE)
  default_keys <- c(
    "StudyOID", "SubjectKey", "ItemGroupRepeatKey", "StudyEventOID"
  )
  domains_order <- topological_sort(config$domains)
  cat("Build order:", paste(domains_order, collapse = " -> "), "\n")
  built_domains <- list()
  for (domain in domains_order) {
    cat(sprintf("Processing domain: %s\n", domain))
    sources <- config$domains[[domain]]
    result_df <- process_domain(
      domain, sources, df_long, default_keys, built_domains
    )
    if (!is.null(result_df) && nrow(result_df) > 0) {
      built_domains[[domain]] <- result_df
      if (!dir.exists(output_dir)) dir.create(output_dir, recursive = TRUE)
      out_path <- file.path(output_dir, paste0(tolower(domain), ".csv"))
      write.csv(result_df, out_path, row.names = FALSE)
    }
  }
  built_domains
}
