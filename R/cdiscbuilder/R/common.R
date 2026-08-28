#' @import dplyr
#' @import lubridate
NULL
#' Calculate SDTM Study Day (--DY)
#'
#' @description Calculates the study
#' day relative to the Reference Start Date (RFSTDTC).
#' There is no Day 0 in SDTM.
#'
#' @param date_series A character or Date vector representing the target dates
#' @param rfstdtc_series A character or Date vector representing
#' the Reference Start Dates
#' @param built_domains Optional list of previously built domains
#' (for contextual derivations)
#' @param ... Additional arguments
#'
#' @return An integer vector containing the calculated study days
#' @export
calculate_study_day <- function(date_series, rfstdtc_series, built_domains = NULL, ...) { # nolint: line_length_linter
  # SDTM Rule:
  # - If date is on or after RFSTDTC: (date - RFSTDTC) + 1
  # - If date is before RFSTDTC: (date - RFSTDTC)
  # - There is no Day 0.
  # Filter out partial dates (length < 10)
  valid_d <- ifelse(nchar(as.character(date_series)) >= 10, date_series, NA_character_) # nolint: line_length_linter
  valid_rf <- ifelse(nchar(as.character(rfstdtc_series)) >= 10, rfstdtc_series, NA_character_) # nolint: line_length_linter
  d <- as.Date(valid_d, optional = TRUE)
  rf <- as.Date(valid_rf, optional = TRUE)
  diff <- as.numeric(d - rf)
  dy <- ifelse(!is.na(diff) & diff >= 0, diff + 1, diff)
  as.integer(dy)
}
#' Extract a specific column from long-format findings data
#'
#' @description Subsets long-format data by Form OID and optionally Item OID,
#' returning a specific column alongside primary keys.
#'
#' @param df_long A data.frame containing the long-format clinical data
#' @param form_oids A character vector of Form OIDs to filter by
#' @param item_oids An optional character vector of Item OIDs to filter by
#' @param return_col The name of the column to return as
#' the value (default: "Value")
#' @param keys A character vector of primary key columns to retain
#'
#' @return A data.frame subsetted by the requested items
#' @export
extract_value <- function(df_long, form_oids, item_oids = NULL, return_col = "Value", keys = NULL) { # nolint: line_length_linter
  if (is.null(form_oids)) form_oids <- character(0)
  subset <- df_long |> filter(FormOID %in% form_oids) # nolint: object_usage_linter
  if (!is.null(item_oids)) {
    subset <- subset |> filter(ItemOID %in% item_oids) # nolint: object_usage_linter
  }
  if (nrow(subset) == 0) {
    tibble::tibble()
  }
  cols_to_keep <- c(keys, return_col)
  cols_to_keep <- intersect(cols_to_keep, names(subset))
  result <- subset |> select(all_of(cols_to_keep))
  result
}
#' Add a specified number of days to a date string
#'
#' @description Safely adds an integer number of days to an ISO 8601 date string. # nolint: line_length_linter
#'
#' @param dtc A character string representing the base date (e.g., "2023-01-01")
#' @param n The number of days to add (can be integer or character)
#'
#' @return A character string with the new date, or an empty string if invalid
#' @export
add_days <- function(dtc, n) {
  d <- tryCatch(as.Date(dtc), error = function(e) NA)
  n <- suppressWarnings(as.integer(n))
  ifelse(is.na(d) | is.na(n), "", format(d + n))
}
#' Determine EPOCH from visit name
#'
#' @description Simple heuristic
#' to determine EPOCH (e.g., SCREENING or TREATMENT)
#' based on the visit name.
#'
#' @param visit A character string representing the visit name
#'
#' @return A character string ("SCREENING" or "TREATMENT")
#' @export
epoch_of <- function(visit) {
  ifelse(visit %in% c("SCREENING", ""), "SCREENING", "TREATMENT")
}
#' Resolve VISIT name and number using a lookup table
#'
#' @description Joins a visit lookup table (TV domain or equivalent) to assign
#' standard VISIT and VISITNUM values based on a visit code.
#'
#' @param df The target data.frame to which visit info will be added
#' @param lookup_df A data.frame containing visit lookup information
#' @param code_col The column name in `df` containing the visit
#' code (default: "VISIT")
#'
#' @return A data.frame with the joined visit information
#' @export
join_visit <- function(df, lookup_df, code_col = "VISIT") {
  vt <- lookup_df |> dplyr::transmute(
    .code = VISITCD, .VISIT = VISIT, # nolint: object_usage_linter
    .VISITNUM = VISITNUM, .OFFSET = OFFSET # nolint: object_usage_linter
  )
  df |>
    dplyr::left_join(vt, by = setNames(".code", code_col)) |>
    dplyr::mutate(VISIT = .VISIT, VISITNUM = .VISITNUM, .OFFSET = .OFFSET) |> # nolint: object_usage_linter
    dplyr::select(-.VISIT, -.VISITNUM)
}
#' Add a sequence variable within USUBJID
#'
#' @description Generates a sequential integer (as
#' character) grouped by USUBJID,
#' commonly used for SDTM --SEQ variables.
#'
#' @param df A data.frame containing a USUBJID column
#' @param seqvar The name of the sequence variable to create (e.g., "VSSEQ")
#'
#' @return A data.frame with the newly created sequence variable
#' @export
add_seq <- function(df, seqvar) {
  df |>
    dplyr::group_by(USUBJID) |> # nolint: object_usage_linter
    dplyr::mutate("{seqvar}" := as.character(dplyr::row_number())) |>
    dplyr::ungroup()
}
