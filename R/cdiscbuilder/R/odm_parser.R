#' @import xml2
#' @import dplyr
#' @import tidyr
#' @import purrr
NULL
DEFAULT_XML_MAPPING <- list( # nolint: object_name_linter
  study_oid = "StudyOID",
  subject_key = "SubjectKey",
  study_subject_id = "StudySubjectID",
  study_event_oid = "StudyEventOID",
  study_event_repeat_key = "StudyEventRepeatKey",
  study_event_start_date = "StartDate",
  form_oid = "FormOID",
  item_group_oid = "ItemGroupOID",
  item_group_repeat_key = "ItemGroupRepeatKey",
  item_oid = "ItemOID",
  value = "Value"
)
#' Parse ODM XML into a long-format DataFrame
#'
#' @param xml_file Path to the ODM XML file
#' @param xml_mapping Optional list mapping to override DEFAULT_XML_MAPPING
#'
#' @return A data.frame (tibble) with the long data
#' @export
parse_odm_to_long_df <- function(xml_file, xml_mapping = NULL) {
  merged_mapping <- DEFAULT_XML_MAPPING
  if (!is.null(xml_mapping)) {
    merged_mapping <- modifyList(merged_mapping, xml_mapping)
  }
  doc <- tryCatch(
    {
      read_xml(xml_file)
    },
    error = function(e) {
      message(sprintf("Error parsing XML file %s: %s", xml_file, e$message))
      NULL
    }
  )
  if (is.null(doc)) {
    return(tibble::tibble())
  }
  ns <- xml_ns(doc)
  # Ensure we have odm namespace. If not explicit, might use d1
  if (!"odm" %in% names(ns)) {
    odm_ns <- as.character(ns)[1] # nolint: object_usage_linter
    ns <- xml_ns(doc)
    # This is a basic fallback, properly one should find the namespace URI
  }
  # 1. Parse Metadata
  item_metadata <- list()
  item_defs <- xml_find_all(doc, ".//d1:ItemDef", ns)
  for (item_def in item_defs) {
    oid <- xml_attr(item_def, "OID")
    name <- xml_attr(item_def, "Name")
    question <- ""
    q_node <- xml_find_first(item_def, "./d1:Question/d1:TranslatedText", ns)
    if (!is.na(q_node)) {
      question <- trimws(xml_text(q_node))
    }
    item_metadata[[oid]] <- list(Question = question, ItemName = name)
  }
  # 2. Extract Data
  data_rows <- list()
  clinical_data_nodes <- xml_find_all(doc, ".//d1:ClinicalData", ns)
  for (cd in clinical_data_nodes) {
    study_oid <- xml_attr(cd, merged_mapping$study_oid)
    subject_data_nodes <- xml_find_all(cd, "./d1:SubjectData", ns)
    for (sd in subject_data_nodes) {
      subject_key <- xml_attr(sd, merged_mapping$subject_key)
      study_subject_id <- xml_attr(sd, merged_mapping$study_subject_id)
      if (is.na(study_subject_id)) study_subject_id <- xml_attr(sd, tolower(merged_mapping$study_subject_id)) # nolint: line_length_linter
      if (is.na(study_subject_id)) study_subject_id <- xml_attr(sd, "StudySubjectID") # nolint: line_length_linter
      if (is.na(study_subject_id)) study_subject_id <- xml_attr(sd, "studysubjectid") # nolint: line_length_linter
      if (is.na(subject_key)) subject_key <- study_subject_id
      study_event_nodes <- xml_find_all(sd, "./d1:StudyEventData", ns)
      for (sed in study_event_nodes) {
        study_event_oid <- xml_attr(sed, merged_mapping$study_event_oid)
        study_event_repeat_key <- xml_attr(sed, merged_mapping$study_event_repeat_key) # nolint: line_length_linter
        if (is.na(study_event_repeat_key)) study_event_repeat_key <- "1"
        start_date <- xml_attr(sed, merged_mapping$study_event_start_date)
        if (is.na(start_date)) start_date <- ""
        form_nodes <- xml_find_all(sed, "./d1:FormData", ns)
        if (length(form_nodes) == 0) {
          # Fallback for ODM v2.0 where FormData is omitted and ItemGroupData is used as the form container # nolint: line_length_linter
          form_nodes <- xml_find_all(sed, "./d1:ItemGroupData", ns)
        }
        for (form in form_nodes) {
          form_oid <- xml_attr(form, merged_mapping$form_oid)
          if (is.na(form_oid)) {
            form_oid <- xml_attr(form, "ItemGroupOID")
          }
          ig_nodes <- xml_find_all(form, "./d1:ItemGroupData", ns)
          for (ig in ig_nodes) {
            item_group_oid <- xml_attr(ig, merged_mapping$item_group_oid)
            item_group_repeat_key <- xml_attr(ig, merged_mapping$item_group_repeat_key) # nolint: line_length_linter
            item_nodes <- xml_find_all(ig, "./d1:ItemData", ns)
            for (item in item_nodes) {
              item_oid <- xml_attr(item, merged_mapping$item_oid)
              value <- xml_attr(item, merged_mapping$value)
              if (is.na(value)) {
                # Try to get value from a child <Value> node
                val_node <- xml_find_first(item, "./d1:Value", ns)
                if (!is.na(val_node)) {
                  value <- xml_text(val_node)
                }
              }
              meta <- item_metadata[[item_oid]]
              if (is.null(meta)) {
                meta <- list(Question = "", ItemName = "")
              }
              data_rows[[length(data_rows) + 1]] <- list(
                StudyOID = study_oid,
                SubjectKey = subject_key,
                StudySubjectID = study_subject_id,
                StudyEventOID = study_event_oid,
                StudyEventRepeatKey = study_event_repeat_key,
                StudyEventStartDate = start_date,
                FormOID = form_oid,
                ItemGroupOID = item_group_oid,
                ItemGroupRepeatKey = item_group_repeat_key,
                ItemOID = item_oid,
                Value = value,
                Question = meta$Question,
                ItemName = meta$ItemName
              )
            }
          }
        }
      }
    }
  }
  df <- bind_rows(data_rows)
  df
}
#' Extract metadata summary
#'
#' @param df_long Long format dataframe
#'
#' @return Dataframe with deduplicated metadata and sample values
#' @export
extract_metadata_summary <- function(df_long) {
  if (is.null(df_long) || nrow(df_long) == 0) {
    return(tibble::tibble(
      FormOID = character(),
      ItemGroupOID = character(),
      ItemOID = character(),
      ItemName = character(),
      Question = character(),
      SampleValues = character()
    ))
  }
  meta_cols <- c("FormOID", "ItemGroupOID", "ItemOID", "ItemName", "Question")
  missing_cols <- setdiff(meta_cols, names(df_long))
  if (length(missing_cols) > 0) {
    stop(
      "Input DataFrame is missing required metadata columns: ",
      paste(missing_cols, collapse = ", ")
    )
  }
  meta_df <- df_long |>
    select(all_of(meta_cols)) |>
    distinct(FormOID, ItemGroupOID, ItemOID, .keep_all = TRUE) |> # nolint: object_usage_linter
    arrange(FormOID, ItemGroupOID, ItemOID)
  # Get sample values
  sample_values_list <- purrr::map_chr(seq_len(nrow(meta_df)), function(i) {
    row <- meta_df[i, ]
    vals <- df_long |>
      filter(
        FormOID == row$FormOID, # nolint: object_usage_linter
        ItemGroupOID == row$ItemGroupOID, # nolint: object_usage_linter
        ItemOID == row$ItemOID, # nolint: object_usage_linter
        !is.na(Value), # nolint: object_usage_linter
        Value != "NA"
      ) |>
      pull(Value) |>
      as.character() |>
      trimws()
    vals <- vals[vals != ""]
    unique_vals <- unique(vals)
    unique_vals <- head(unique_vals, 3)
    if (length(unique_vals) == 0) {
      "[]"
    } else {
      paste0("['", paste(unique_vals, collapse = "', '"), "']")
    }
  })
  meta_df$SampleValues <- sample_values_list
  meta_df
}
