# Standardize morphology results with SDTMIG 3.3

Goal: Preserve the collected cell type result and report its standardized
form in the morphology domain.

Input: Cell type findings and collection dates from a morphology form.

Variables:

- `MOSEQ` identifies a finding within a subject.
- `MOTESTCD` and `MOTEST` identify the cell type assessment.
- `MOORRES` retains the collected result.
- `MOSTRESC` reports NORMAL or ABNORMAL.
- `MODTC` retains the collection date.

Note: `project.yaml` declares SDTMIG 3.3. The data-definition document
records that guide and points to the SAS Transport version 5 dataset.
