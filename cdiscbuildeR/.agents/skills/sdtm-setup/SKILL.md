---
name: sdtm-setup
description: Scaffolds a new SDTM study folder structure and pipeline scripts for cdiscbuildeR. Use this when the user asks to setup a new study or runs /sdtm-setup.
---
# SDTM Setup Skill

When the user asks to set up a new study, or uses the command `/sdtm-setup <study_name>`, you must autonomously execute the following steps to scaffold the project for `cdiscbuildeR`:

1. **Create Directory Structure**:
   Create the following folders in the target study directory (`<study_name>`):
   - `adam/`
   - `edc/`
   - `odm/`
   - `sdtm/specs/`
   - `tlf/`

2. **Move Raw Data & Metadata**:
   If there is an `odm.xml` or other raw data file, move it into `odm/odm.xml`. If there are CRF description markdown files (e.g. `CRF_spec.md`), move them to `odm/CRF_spec.md`.

3. **Install Dependencies**:
   Run an R script to ensure all required tidyverse and package dependencies are installed in the user's library:
   `install.packages(c("tidyr", "stringr", "purrr", "rlang", "sqldf"), lib = Sys.getenv("R_LIBS_USER"))`

4. **Copy Schema Templates**:
   Copy the default SDTM schema from the `cdiscbuildeR` package (e.g., from `inst/templates/special_purpose_domain_template.yaml`) into the `<study_name>/sdtm/specs/dm.yaml` folder.

5. **Create Pipeline Script**:
   Create a standard `run_sdtm_pipeline.R` script in the `<study_name>/sdtm/` folder with the following template:

   ```R
   # sdtm/run_sdtm_pipeline.R
   library(cdiscbuildeR)
   library(readr)
   library(dplyr)
   library(tidyr)
   library(stringr)
   library(purrr)
   library(rlang)

   raw_xml_path <- "odm/odm.xml"
   intermediate_csv_path <- "odm/long_data.csv"
   specs_dir <- "sdtm/specs"
   sdtm_output_dir <- "sdtm/output"

   cat("Step 1: Parsing ODM XML to long-format data...\n")
   long_df <- cdiscbuildeR:::parse_odm_to_long_df(raw_xml_path)
   write_csv(long_df, intermediate_csv_path)

   cat("Step 2: Generating SDTM datasets from specifications...\n")
   sdtm_datasets <- cdiscbuildeR:::create_sdtm_datasets(specs_dir, intermediate_csv_path, sdtm_output_dir)
   cat("Pipeline complete!\n")
   ```

6. **Inform the User**:
   Notify the user that the folder structure, schema, dependencies, and `sdtm/run_sdtm_pipeline.R` have been set up, and they can proceed to edit the YAML specs or run the pipeline.
