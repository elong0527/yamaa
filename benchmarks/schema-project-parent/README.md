# Share One Study's Project Between Two Submission Packages

**Goal:** carry the planned treatment (`TRT01P`) and the reason for
discontinuation from the study (`DCSREAS`) into ADSL for two submission
packages of one study, with the second package's data-definition document.

**Input:** `spec.yaml` reads one pre-analysis row per subject, and three
project files compose:

- `study/project.yaml` holds what every package of the study shares: the
  study, its implementation guide, the vendor that prepared the metadata,
  the specification and `study/codelists.yaml`, the study's coded values;
- `pilot3/project.yaml`, the first package, names the study's project as its
  parent and adds only where its document is written;
- `project.yaml`, the second package, names the study's project as its
  parent too, lists the study's coded values and then `codelists.yaml`, its
  own discontinuation reasons, and removes the vendor, since the sponsor
  submits this package itself.

**Variables:**

- `TRT01P` is the planned treatment, one of the study's coded values.
- `DCSREAS` is the reason for discontinuation from the study.

**Note:** each package keeps every section of the study's project and
changes only what it writes, field by field; a list it writes replaces the
study's whole. A file the study's project names is read beside that project,
so both packages build the one specification it lists. The second package's
document cites the study and its guide from the study's project, decodes
each reason as the package's coded values do, and names no originator.
