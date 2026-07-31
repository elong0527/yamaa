library(xml2)
library(yaml)
library(readr)
library(dplyr)
library(tidyr)
library(stringr)
library(purrr)
library(rlang)
library(cdiscbuildeR)

# Paths based on the new standardized folder structure
raw_xml_path <- "odm/odm.xml"
specs_dir <- "sdtm/specs"
output_dir <- "sdtm/output"
temp_long_data_path <- "odm/long_data.csv"

# Make sure output directory exists
dir.create(output_dir, showWarnings = FALSE, recursive = TRUE)

cat("Step 1: Parsing ODM XML to long-format data...\n")
long_data <- cdiscbuildeR:::parse_odm_to_long_df(raw_xml_path)
write_csv(long_data, temp_long_data_path)
cat("Intermediate data saved to:", temp_long_data_path, "\n\n")

cat("Step 2: Generating SDTM datasets from YAML specifications...\n")
sdtm_datasets <- cdiscbuildeR:::create_sdtm_datasets(specs_dir, temp_long_data_path, output_dir)

cat("\nPipeline complete! SDTM datasets generated in:", output_dir, "\n")
