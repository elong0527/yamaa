# Column Composition

**Goal:** demonstrate a study level changing details inside inherited
columns - `AVAL`, `ANRIND`, and `PARCAT1`.

**Input:** three spec files, read root-first:

- `spec_organization.yaml` turns the laboratory test code, result, and
  reference range indicator into `PARAMCD`, `AVAL`, `ANRIND`, and
  `PARCAT1`, reading the indicator without regard to letter case; it
  marks `AVAL` as the analysis value and checks that it is not negative;
- `spec_compound.yaml` adds the low reference range wording the
  organization does not list, and notes that `AVAL` comes from the
  central laboratory;
- `spec_study.yaml` is the entry: it applies one corrected result,
  marks `AVAL` as adjudicated and checks that it is never missing,
  leaves an unrecognized indicator empty, states `PARCAT1` as the one
  category this study collects, and declares the complete output.

Each level names the one level above it, so the chain reads
organization, then compound, then study, and a later level wins where
two levels state the same detail.

**Note:** a later level composes a column detail by detail. The added
reference range wording joins the two the organization already lists,
the three annotations on `AVAL` accumulate, and the case-insensitive
reading survives a level that never mentions it. A level naming a
different way to produce a value replaces it whole instead, which is
why `PARCAT1` stops reading the test code. Lists never accumulate: the
study's own check on `AVAL` replaces the organization's.
