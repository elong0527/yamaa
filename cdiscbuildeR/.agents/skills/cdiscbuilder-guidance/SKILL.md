---
name: cdiscbuilder-guidance
description: Provides guidelines for folder structure and data placement when working with the cdiscbuildeR package. Use this to organize raw EDC data, specs, scripts, and output data correctly.
---
# cdiscbuildeR Guidance

When interacting with a project that uses the `cdiscbuildeR` R package, you must enforce the standard 5-folder structure and ensure data files are placed correctly. 

## Standard Folder Structure

- `adam/`: Analysis datasets.
- `edc/`: EDC-related configurations, generators, and metadata.
- `odm/`: Must contain all source data files, including EDC XML extracts (e.g., `odm.xml`) and CRF description markdown files (e.g., `CRF_spec.md`). Never put raw data at the root of the project.
- `sdtm/`: Contains everything related to SDTM generation:
  - `sdtm/specs/`: The directory for YAML configurations that define SDTM domains (using templates from `cdiscbuildeR/inst/templates`).
  - `sdtm/output/`: The output directory for the final generated SDTM datasets.
  - `sdtm/run_sdtm_pipeline.R`: The main script to generate SDTM datasets.
- `tlf/`: Tables, Listings, and Figures.

## Best Practices
1. **Raw Data Placement**: Immediately move any `odm.xml` or raw data extracts to `odm/` upon discovery. Do not leave them in the root directory.
2. **Data Pipeline**: 
    - **Step 1 (Parse XML)**: Parse XML using `cdiscbuildeR:::parse_odm_to_long_df`. Save intermediate data to `odm/`.
    - **Step 2 (Specifications)**: Create dataset specifications in `sdtm/specs/`.
      - **Circular Dependencies**: If SDTM domains have circular dependencies (e.g. DM needs dates from EX, but EX needs DM's RFSTDTC), break the cycle by creating a hidden reference domain prefixed with an underscore (e.g. `_DM_REF`). The engine builds these first without topological sort constraints, allowing downstream domains to safely pull values via `_DM_REF.COLUMN`.
    - **Step 3 (SDTM Generation)**: Run the SDTM engine (`cdiscbuildeR:::create_sdtm_datasets`) to output domains to `sdtm/output/`.
3. **Domain Types and Schemas**: 
    - The engine defaults to the `general_domain` schema for Special Purpose (e.g., DM), Interventions (e.g., EX, CM), and Events (e.g., AE, DS). These are "horizontal" domains.
    - For Findings domains (e.g., FA, LB, VS, QS), which are "vertical", you MUST explicitly declare `type: FINDINGS` at the root of the domain's YAML configuration to use the `findings_domain` schema.
4. **Custom Functions (R Package)**:
    - Custom calculation functions called by YAML specs (`function_`) must be placed in the `R/` directory of the `cdiscbuildeR` package and exported (`#' @export`).
    - **Do NOT combine all functions into a single file.** Distribute them into logical, domain-specific files (e.g., `dm_functions.R`, `vs_functions.R`) with cross-domain utilities residing in `common.R`.
