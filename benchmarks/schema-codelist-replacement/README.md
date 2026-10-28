# Replace One of a Study's Shared Codelists in a Submission Package

**Goal:** carry the planned treatment (`TRT01P`) and the reason for
discontinuation from the study (`DCSREAS`) into ADSL, with a data-definition
document listing the coded values and decodes of each.

**Input:** `spec.yaml` reads one pre-analysis row per subject, and
`project.yaml` lists two codelist files in order:

- `study/codelists.yaml` holds the study's lists of planned treatments and
  of discontinuation reasons, which every submission package of the study
  shares;
- `codelists.yaml` holds this package's own list of discontinuation reasons,
  under the study's code for it.

**Variables:**

- `TRT01P` is the planned treatment, one of the study's coded values.
- `DCSREAS` is the reason for discontinuation from the study, one of the
  package's coded values, which admit a subject lost to follow-up.

**Note:** a list in a later file replaces the earlier list of the same code
whole: its name, its values and its decodes come from the package, and none
from the study, so a subject lost to follow-up is admitted and each reason
decodes as the coded value itself. The study's planned treatments are kept
as the study declares them. The data-definition document lists the kept
planned treatments first, where the study's file lists them, then the
package's reasons, where the package's file lists them.
