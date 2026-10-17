# Spec Inheritance

**Goal:** show how a study spec inherits through its compound level
from its organization level: shared levels carry the common mapping
once, and the study level wins on final wording.

**Dataset:** `adlb.csv`, ADLB analysis dataset. One record per
subject per laboratory parameter.

**Input:** three spec files, read root-first:

- `spec_organization.yaml` maps the laboratory test code, result,
  and unit into `PARAMCD`, `AVAL`, and `AVALU`;
- `spec_compound.yaml` adds field types and retitles the AVAL
  result;
- `spec_study.yaml` is the entry: it names the compound level as
  its parent, restates the AVALU unit wording, and declares the
  complete output.

Each level names the one level above it, so the chain reads
organization, then compound, then study, and a later level wins
where two levels state the same detail.
