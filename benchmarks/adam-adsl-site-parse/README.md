# Parse the Site from the Subject Identifier

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-site-parse.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the parsed site, the site to use, and the display
reference (`SITEIDP`, `SITEID`, and `SUBJREF`) for each subject.

**Input:** demographics with the study identifier, the unique subject
identifier, the collected subject number, and the collected site.

**Variables:**

- `SITEIDP` is the site code read from the middle of the unique
  subject identifier when it has the form `YAMAA-<site>-<four digits>`;
  empty when the identifier has any other shape.
- `SITEID` is the site to use: the parsed site when present, otherwise
  the collected site, otherwise `UNKNOWN`.
- `SUBJREF` combines the site to use and the collected subject number
  with a colon; `UNKNOWN` when the subject number is missing.

**Note:** keeping both the parsed and the final site shows which
subjects fell back to the collected site and which had no usable site
at all.

**Standard:** ADaM | **Domain:** ADSL
