# Inherit laboratory-to-analysis renaming from organization through study

**Goal:** teach organization-to-compound-to-study inheritance, using
renaming of laboratory (LB) results into analysis (ADLB) results as the
vehicle.

**Input:** one row per laboratory test result, with test code,
standardized result, and standardized unit.

**Variables:**

- **PARAMCD**: the test code being analyzed; its final wording comes
  from the organization level.
- **AVAL**: the standardized numeric result; its final wording comes
  from the study level after the compound level retitles it.
- **AVALU**: the standardized unit for the result; its final wording
  comes from the study level.

**Note:** the shared organization and compound levels carry the common
renaming, and the deepest study level wins on final wording.

**Standard:** ADaM | **Domain:** ADLB
