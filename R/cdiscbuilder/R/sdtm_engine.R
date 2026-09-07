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
        missing_target_keys <- setdiff(merge_keys, names(final_df))
        if (length(missing_target_keys) > 0) {
          stop(
            "Lookup keys not found in target dataset: ",
            paste(missing_target_keys, collapse = ", ")
          )
        }
        missing_reference_keys <- setdiff(merge_keys, names(ref_df))
        if (length(missing_reference_keys) > 0) {
          stop(
            "Lookup keys not found in ", ref_domain, ": ",
            paste(missing_reference_keys, collapse = ", ")
          )
        }
        if (anyDuplicated(ref_df[merge_keys]) > 0) {
          stop(
            "Lookup source ", ref_domain,
            " has multiple matches for keys: ",
            paste(merge_keys, collapse = ", ")
          )
        }
        ref_subset <- ref_df |>
          select(all_of(c(merge_keys, ref_col)))
        merged <- final_df |>
          select(all_of(merge_keys)) |>
          left_join(ref_subset, by = merge_keys)
        series <- merged[[ref_col]]
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
.read_delimited_source <- function(path) {
  connection <- file(path, "rb")
  on.exit(close(connection))
  size <- file.info(path)$size
  bytes <- readBin(connection, what = "raw", n = size)
  fail <- function(code, record = record_number, field = field_number) {
    stop(
      sprintf("%s: %s at record %d, field %d", code, path, record, field),
      call. = FALSE
    )
  }
  if (length(bytes) == 0) {
    fail("source_header_absent", 1L, 1L)
  }
  if (
    length(bytes) >= 3 &&
      identical(bytes[seq_len(3)], as.raw(c(0xef, 0xbb, 0xbf)))
  ) {
    fail("source_byte_order_mark", 1L, 1L)
  }
  invalid_pos <- {
    n <- length(bytes)
    pos <- NA_integer_
    i <- 1L
    while (i <= n && is.na(pos)) {
      b <- as.integer(bytes[i])
      if (b <= 0x7fL) {
        i <- i + 1L
        next
      }
      expected <- if (b >= 0xc2L && b <= 0xdfL) 1L else if (b >= 0xe0L && b <= 0xefL) 2L else if (b >= 0xf0L && b <= 0xf4L) 3L else NA_integer_
      if (is.na(expected)) {
        pos <- i
        break
      }
      if (i + expected > n) {
        pos <- i
        break
      }
      continuation_ok <- TRUE
      for (k in seq_len(expected)) {
        cb <- as.integer(bytes[i + k])
        if (cb < 0x80L || cb > 0xbfL) {
          continuation_ok <- FALSE
          break
        }
      }
      if (!continuation_ok) {
        pos <- i
        break
      }
      if (expected == 2L) {
        b2 <- as.integer(bytes[i + 1L])
        if (b == 0xe0L && b2 < 0xa0L) pos <- i
        if (b == 0xedL && b2 > 0x9fL) pos <- i
      } else if (expected == 3L) {
        b2 <- as.integer(bytes[i + 1L])
        if (b == 0xf0L && b2 < 0x90L) pos <- i
        if (b == 0xf4L && b2 > 0x8fL) pos <- i
      }
      if (!is.na(pos)) break
      i <- i + expected + 1L
    }
    pos
  }
  if (!is.na(invalid_pos)) {
    scan_record <- 1L
    scan_field <- 1L
    scan_state <- "start"
    scan_index <- 1L
    while (scan_index < invalid_pos) {
      b <- as.integer(bytes[scan_index])
      if (scan_state == "quoted") {
        if (b == 0x22L) {
          if (
            scan_index + 1L < invalid_pos &&
              as.integer(bytes[scan_index + 1L]) == 0x22L
          ) {
            scan_index <- scan_index + 2L
            next
          }
          scan_state <- "after_quote"
          scan_index <- scan_index + 1L
          next
        }
        scan_index <- scan_index + 1L
        next
      }
      if (b == 0x0dL) {
        if (
          scan_index + 1L < invalid_pos &&
            as.integer(bytes[scan_index + 1L]) == 0x0aL
        ) {
          scan_record <- scan_record + 1L
          scan_field <- 1L
          scan_state <- "start"
          scan_index <- scan_index + 2L
          next
        }
        if (scan_index + 1L == invalid_pos) {
          scan_index <- scan_index + 1L
          next
        }
        scan_index <- scan_index + 1L
        next
      }
      if (b == 0x0aL) {
        scan_record <- scan_record + 1L
        scan_field <- 1L
        scan_state <- "start"
        scan_index <- scan_index + 1L
        next
      }
      if (scan_state == "start") {
        if (b == 0x22L) {
          scan_state <- "quoted"
        } else if (b == 0x2cL) {
          scan_field <- scan_field + 1L
        } else {
          scan_state <- "bare"
        }
        scan_index <- scan_index + 1L
        next
      }
      if (scan_state == "bare") {
        if (b == 0x2cL) {
          scan_field <- scan_field + 1L
          scan_state <- "start"
        }
        scan_index <- scan_index + 1L
        next
      }
      if (b == 0x2cL) {
        scan_field <- scan_field + 1L
        scan_state <- "start"
      }
      scan_index <- scan_index + 1L
    }
    fail("invalid_text", scan_record, scan_field)
  }
  text <- tryCatch(
    iconv(
      rawToChar(bytes),
      from = "UTF-8",
      to = "UTF-8",
      sub = NA_character_
    ),
    error = function(error) NA_character_
  )
  if (is.na(text)) {
    fail("invalid_text", 1L, 1L)
  }
  code_points <- utf8ToInt(text)
  records <- list()
  current_record <- list()
  field_text <- integer()
  field_quoted <- FALSE
  state <- "start"
  record_number <- 1L
  field_number <- 1L
  append_field <- function() {
    current_record[[length(current_record) + 1L]] <<- list(
      text = intToUtf8(field_text),
      quoted = field_quoted
    )
    field_text <<- integer()
    field_quoted <<- FALSE
  }
  append_record <- function() {
    append_field()
    records[[length(records) + 1L]] <<- current_record
    current_record <<- list()
    record_number <<- record_number + 1L
    field_number <<- 1L
  }
  index <- 1L
  while (index <= length(code_points)) {
    code_point <- code_points[[index]]
    if (state == "quoted") {
      if (code_point == 13L) {
        fail("source_carriage_return")
      }
      if (code_point == 34L) {
        if (
          index < length(code_points) &&
            code_points[[index + 1L]] == 34L
        ) {
          field_text <- c(field_text, 34L)
          index <- index + 2L
        } else {
          state <- "after_quote"
          index <- index + 1L
        }
      } else {
        field_text <- c(field_text, code_point)
        index <- index + 1L
      }
      next
    }
    if (code_point == 13L) {
      if (
        index == length(code_points) ||
          code_points[[index + 1L]] != 10L
      ) {
        fail("source_carriage_return")
      }
      append_record()
      state <- "start"
      index <- index + 2L
      next
    }
    if (code_point == 10L) {
      append_record()
      state <- "start"
      index <- index + 1L
      next
    }
    if (state == "start") {
      if (code_point == 34L) {
        field_quoted <- TRUE
        state <- "quoted"
      } else if (code_point == 44L) {
        append_field()
        field_number <- field_number + 1L
      } else {
        field_text <- c(field_text, code_point)
        state <- "bare"
      }
      index <- index + 1L
      next
    }
    if (state == "bare") {
      if (code_point == 34L) {
        fail("source_quote_in_bare_field")
      }
      if (code_point == 44L) {
        append_field()
        field_number <- field_number + 1L
        state <- "start"
      } else {
        field_text <- c(field_text, code_point)
      }
      index <- index + 1L
      next
    }
    if (code_point != 44L) {
      fail("source_text_after_quote")
    }
    append_field()
    field_number <- field_number + 1L
    state <- "start"
    index <- index + 1L
  }
  if (state == "quoted") {
    fail("source_quote_unterminated")
  }
  if (tail(code_points, 1L) != 10L) {
    append_record()
  }
  header <- vapply(records[[1L]], `[[`, character(1), "text")
  empty_name <- which(header == "")[1L]
  if (!is.na(empty_name)) {
    fail("source_field_name_empty", 1L, empty_name)
  }
  duplicate_name <- anyDuplicated(header)
  if (duplicate_name != 0L) {
    fail("source_field_name_duplicate", 1L, duplicate_name)
  }
  data_records <- records[-1L]
  for (record_index in seq_along(data_records)) {
    if (length(data_records[[record_index]]) != length(header)) {
      fail(
        "source_record_width",
        record_index + 1L,
        min(length(data_records[[record_index]]), length(header)) + 1L
      )
    }
  }
  columns <- lapply(seq_along(header), function(column_index) {
    vapply(data_records, function(record) {
      field <- record[[column_index]]
      if (!field$quoted && identical(field$text, "")) {
        NA_character_
      } else {
        field$text
      }
    }, character(1))
  })
  names(columns) <- header
  as.data.frame(
    columns,
    stringsAsFactors = FALSE,
    check.names = FALSE,
    optional = TRUE
  )
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
  df_long <- .read_delimited_source(input_csv)
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
