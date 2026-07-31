#' Core logic to find dose dates
#' @noRd
.get_dose_date <- function(usubjid_series, built_domains, mode = "first", ...) {
  if (is.null(built_domains)) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  kwargs <- list(...)
  ex_dose_col <- if (!is.null(kwargs$ex_dose_col)) kwargs$ex_dose_col else "EXDOSE" # nolint: line_length_linter
  ec_dose_col <- if (!is.null(kwargs$ec_dose_col)) kwargs$ec_dose_col else "ECDOSE" # nolint: line_length_linter
  dfs <- list()
  if (!is.null(built_domains$EX) && nrow(built_domains$EX) > 0 &&
    "USUBJID" %in% names(built_domains$EX)) { # nolint: indentation_linter
    ex_df <- built_domains$EX
    date_col <- if (mode == "first") "EXSTDTC" else "EXENDTC"
    if (date_col %in% names(ex_df)) {
      if (ex_dose_col %in% names(ex_df)) {
        ex_df <- ex_df |> filter(as.numeric(.data[[ex_dose_col]]) > 0)
      }
      dfs[[length(dfs) + 1]] <- ex_df |> select(USUBJID, DATE = all_of(date_col)) # nolint: object_usage_linter, line_length_linter
    }
  }
  if (!is.null(built_domains$EC) && nrow(built_domains$EC) > 0 &&
    "USUBJID" %in% names(built_domains$EC)) { # nolint: indentation_linter
    ec_df <- built_domains$EC
    date_col <- if (mode == "first") "ECSTDTC" else "ECENDTC"
    if (date_col %in% names(ec_df)) {
      if (ec_dose_col %in% names(ec_df)) {
        ec_df <- ec_df |> filter(as.numeric(.data[[ec_dose_col]]) > 0)
      }
      dfs[[length(dfs) + 1]] <- ec_df |> select(USUBJID, DATE = all_of(date_col)) # nolint: object_usage_linter, line_length_linter
    }
  }
  if (length(dfs) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  combined <- bind_rows(dfs) |> filter(!is.na(DATE)) # nolint: object_usage_linter
  if (nrow(combined) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  combined <- combined |>
    mutate(
      DATETIME = suppressWarnings(
        as.POSIXct(DATE, format = "%Y-%m-%dT%H:%M", tz = "UTC") # nolint: object_usage_linter
      )
    ) |>
    mutate(
      DATETIME = if_else(
        is.na(DATETIME), # nolint: object_usage_linter
        suppressWarnings(as.POSIXct(DATE, format = "%Y-%m-%d", tz = "UTC")),
        DATETIME
      )
    ) |>
    filter(!is.na(DATETIME))
  if (nrow(combined) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  if (mode == "first") {
    res <- combined |>
      group_by(USUBJID) |> # nolint: object_usage_linter
      slice_min(DATETIME, n = 1, with_ties = FALSE) |> # nolint: object_usage_linter
      ungroup()
  } else {
    res <- combined |>
      group_by(USUBJID) |> # nolint: object_usage_linter
      slice_max(DATETIME, n = 1, with_ties = FALSE) |> # nolint: object_usage_linter
      ungroup()
  }
  # Map back to series
  res_map <- setNames(res$DATE, res$USUBJID)
  unname(res_map[as.character(usubjid_series)])
}
#' Get the first dose date for subjects
#'
#' @description Finds the earliest recorded dose date for each subject
#' by looking at
#' EX and EC domains where the dose > 0.
#'
#' @param usubjid_series A character vector of USUBJID values
#' @param built_domains A list of already built SDTM domains
#' (expected to contain EX or EC)
#' @param ... Additional arguments passed to the underlying date parser
#' (e.g. ex_dose_col)
#'
#' @return A character vector of ISO 8601 datetimes corresponding to the first dose # nolint: line_length_linter
#' @export
get_first_dose_date <- function(usubjid_series, built_domains = NULL, ...) {
  .get_dose_date(usubjid_series, built_domains, mode = "first", ...)
}
#' Get the last dose date for subjects
#'
#' @description Finds the latest recorded dose date for each subject
#' by looking at
#' EX and EC domains where the dose > 0.
#'
#' @param usubjid_series A character vector of USUBJID values
#' @param built_domains A list of already built SDTM domains
#' (expected to contain EX or EC)
#' @param ... Additional arguments passed to the underlying date parser
#' (e.g. ex_dose_col)
#'
#' @return A character vector of ISO 8601 datetimes corresponding to the last
#' dose
#' @export
get_last_dose_date <- function(usubjid_series, built_domains = NULL, ...) {
  .get_dose_date(usubjid_series, built_domains, mode = "last", ...)
}
#' Get the earliest informed consent date
#'
#' @description Finds the earliest recorded informed consent
#' date by checking the DS domain
#' (or raw EDC data in raw_mode) for specific consent terms.
#'
#' @param usubjid_series A character vector of USUBJID values
#' @param built_domains A list of already built SDTM domains
#' (expected to contain DS)
#' @param df_long Optional raw EDC data in long format (used if raw_mode = TRUE)
#' @param ... Additional arguments such as raw_mode, consent_terms,
#' term_col, and date_col
#'
#' @return A character vector of ISO 8601 dates/datetimes corresponding
#' to consent
#' @export
get_earliest_informed_consent_date <- function(usubjid_series, built_domains = NULL, df_long = NULL, ...) { # nolint: object_length_linter, line_length_linter
  kwargs <- list(...)
  raw_mode <- if (!is.null(kwargs$raw_mode)) kwargs$raw_mode else FALSE
  consent_terms <- if (!is.null(kwargs$consent_terms)) kwargs$consent_terms else c("INFORMED CONSENT OBTAINED") # nolint: line_length_linter
  upper_terms <- toupper(consent_terms)
  if (raw_mode) {
    if (is.null(df_long) || nrow(df_long) == 0) {
      return(rep(NA_character_, length(usubjid_series)))
    }
    term_col <- if (!is.null(kwargs$term_col)) kwargs$term_col else "DSTERM"
    date_col <- if (!is.null(kwargs$date_col)) kwargs$date_col else "DSSTDAT"
    formoid <- kwargs$raw_formoid
    subset <- df_long
    if (!is.null(formoid)) subset <- subset |> filter(FormOID == formoid) # nolint: object_usage_linter
    term_mask <- subset$ItemOID == term_col & toupper(as.character(subset$Value)) %in% upper_terms # nolint: line_length_linter
    consent_subjects <- unique(subset$SubjectKey[term_mask])
    valid <- subset |> filter(ItemOID == date_col, SubjectKey %in% consent_subjects) # nolint: object_usage_linter, line_length_linter
    if (nrow(valid) == 0) {
      return(rep(NA_character_, length(usubjid_series)))
    }
    valid <- valid |>
      mutate(DATETIME = suppressWarnings(as.POSIXct(Value, format = "%Y-%m-%dT%H:%M", tz = "UTC"))) |> # nolint: object_usage_linter, line_length_linter
      mutate(DATETIME = if_else(is.na(DATETIME), suppressWarnings(as.POSIXct(Value, format = "%Y-%m-%d", tz = "UTC")), DATETIME)) |> # nolint: object_usage_linter, line_length_linter
      filter(!is.na(DATETIME))
    if (nrow(valid) == 0) {
      return(rep(NA_character_, length(usubjid_series)))
    }
    res <- valid |>
      group_by(SubjectKey) |> # nolint: object_usage_linter
      slice_min(DATETIME, n = 1, with_ties = FALSE) |> # nolint: object_usage_linter
      ungroup()
    subj_keys_from_map <- sapply(strsplit(as.character(res$SubjectKey), "-"), tail, 1) # nolint: line_length_linter
    res_map <- setNames(res$Value, subj_keys_from_map)
    subj_keys <- sapply(strsplit(as.character(usubjid_series), "-"), tail, 1)
    return(unname(res_map[subj_keys]))
  }
  if (is.null(built_domains) || is.null(built_domains$DS) || nrow(built_domains$DS) == 0) { # nolint: line_length_linter
    return(rep(NA_character_, length(usubjid_series)))
  }
  ds <- built_domains$DS
  term_col <- if (!is.null(kwargs$term_col)) kwargs$term_col else "DSDECOD"
  date_col <- if (!is.null(kwargs$date_col)) kwargs$date_col else "DSSTDTC"
  if (!(term_col %in% names(ds)) || !(date_col %in% names(ds))) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  valid <- ds |> filter(toupper(as.character(.data[[term_col]])) %in% upper_terms) # nolint: line_length_linter
  if (nrow(valid) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  valid <- valid |>
    mutate(
      DATETIME = suppressWarnings(
        as.POSIXct(.data[[date_col]], format = "%Y-%m-%dT%H:%M", tz = "UTC")
      )
    ) |>
    mutate(
      DATETIME = if_else(
        is.na(DATETIME), # nolint: object_usage_linter
        suppressWarnings(
          as.POSIXct(.data[[date_col]], format = "%Y-%m-%d", tz = "UTC")
        ),
        DATETIME
      )
    ) |>
    filter(!is.na(DATETIME))
  if (nrow(valid) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  res <- valid |>
    group_by(USUBJID) |> # nolint: object_usage_linter
    slice_min(DATETIME, n = 1, with_ties = FALSE) |> # nolint: object_usage_linter
    ungroup()
  res_map <- setNames(res[[date_col]], res$USUBJID)
  unname(res_map[as.character(usubjid_series)])
}
#' Get the last participation date
#'
#' @description Finds the latest date across several key SDTM domains
#' (DS, EX, EC, AE, SV)
#' to determine a subject's last known date of participation in the study.
#'
#' @param usubjid_series A character vector of USUBJID values
#' @param built_domains A list of already built SDTM domains
#' @param ... Additional arguments (e.g. domain_dates list to override defaults)
#'
#' @return A character vector of ISO 8601 datetimes representing the last
#' participation
#' @export
get_last_participation_date <- function(usubjid_series, built_domains = NULL, ...) { # nolint: line_length_linter
  if (is.null(built_domains)) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  kwargs <- list(...)
  domain_dates <- kwargs$domain_dates
  if (is.null(domain_dates)) {
    domain_dates <- list(
      DS = "DSSTDTC",
      EX = c("EXSTDTC", "EXENDTC"),
      EC = c("ECSTDTC", "ECENDTC"),
      AE = c("AESTDTC", "AEENDTC"),
      SV = c("SVSTDTC", "SVENDTC")
    )
  }
  dfs <- list()
  for (domain in names(domain_dates)) {
    df <- built_domains[[domain]]
    if (!is.null(df) && nrow(df) > 0 && "USUBJID" %in% names(df)) {
      for (col in domain_dates[[domain]]) {
        if (col %in% names(df)) {
          valid <- df |>
            select(USUBJID, DATE = all_of(col)) |> # nolint: object_usage_linter
            filter(!is.na(DATE)) # nolint: object_usage_linter
          dfs[[length(dfs) + 1]] <- valid
        }
      }
    }
  }
  if (length(dfs) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  combined <- bind_rows(dfs)
  combined <- combined |>
    mutate(
      DATETIME = suppressWarnings(
        as.POSIXct(DATE, format = "%Y-%m-%dT%H:%M", tz = "UTC") # nolint: object_usage_linter
      )
    ) |>
    mutate(
      DATETIME = if_else(
        is.na(DATETIME), # nolint: object_usage_linter
        suppressWarnings(as.POSIXct(DATE, format = "%Y-%m-%d", tz = "UTC")),
        DATETIME
      )
    ) |>
    filter(!is.na(DATETIME))
  if (nrow(combined) == 0) {
    return(rep(NA_character_, length(usubjid_series)))
  }
  res <- combined |>
    group_by(USUBJID) |> # nolint: object_usage_linter
    slice_max(DATETIME, n = 1, with_ties = FALSE) |> # nolint: object_usage_linter
    ungroup()
  res_map <- setNames(res$DATE, res$USUBJID)
  unname(res_map[as.character(usubjid_series)])
}
#' Calculate RFENDTC from RFSTDTC and DSSTDY
#'
#' @description Derives the Reference End Date (RFENDTC) by adding the Disposition Study Day # nolint: line_length_linter
#' (DSSTDY) minus 1 to the Reference Start Date (RFSTDTC).
#'
#' @param rfstdtc A character vector of Reference Start Dates
#' @param dsstdy A character or numeric vector of Disposition Study Days
#' @param built_domains Optional list of built domains
#' @param ... Additional arguments
#'
#' @return A character vector of derived Reference End Dates
#' @export
calc_rfendtc <- function(rfstdtc, dsstdy, built_domains = NULL, ...) {
  add_days(rfstdtc, suppressWarnings(as.integer(dsstdy)) - 1L)
}
