# Describe the Analysis Behind Each Table of a Report

**Goal:** submit a subject-level and a laboratory analysis dataset with one
data-definition document that also describes the analysis behind two tables
of the study report.

**Input:** two spec files, the project that lists them and one file of
analysis results:

- `spec_adsl.yaml` builds the subject-level dataset (ADSL) from
  demographics, one record per subject;
- `spec_adlb.yaml` builds the laboratory dataset (ADLB) from standardized
  laboratory results, one record per subject and test;
- `project.yaml` lists both, declares the analysis plan, the tables document
  and a program as supporting documents, and describes the table of subject
  counts itself;
- `arm/efficacy.yaml`, which the project names first, describes the glucose
  table.

**Variables:**

- `SAFFL` is `Y` for a subject who received a study treatment and `N`
  otherwise.
- `TRT01AN` numbers the treatment the subject received; missing for a subject
  who received none.
- `PARAMCD` is the laboratory test code: glucose, creatinine and the
  pregnancy test.
- `PARAM` names the parameter, with its unit for the numeric tests.
- `AVAL` is the numeric result; missing for the pregnancy test.
- `AVALC` is the result as text.

**Note:** the data-definition document describes each table once, in the
order the project names them, whether a table is described in the project or
in a file of its own: its name and where it is in the tables document, then
each result it shows with the reason and purpose of the analysis, the
datasets it reads, the records it selects from each and the variables it
analyses. The glucose table reads both datasets, so it says how they are
joined, points to the section of the analysis plan that describes it and
carries its program; the subject-level dataset only selects its subjects, so
no variable of it is analysed. Because that table summarises laboratory
parameters, its result names the parameter variable; the count of subjects
reads the subject-level dataset alone and names none, and cites its program
as a document. Every dataset, variable and document a table names is one the
study describes.
