# Find the first dose ending in the second day band

**Goal:** give each subject the sequence of their first dose ending in day
band 2 (`B2SEQ`).

**Input:** alternative specifications share subjects, dose starts and
lengths, and inclusive day bands. The self-join reads the dose file again
under a second input identifier.

**Variables:**

- `B2SEQ` is the earliest dose sequence ending in band 2, and blank when no
  dose ends there. In the self-join it is the earliest dose whose most
  recent earlier dose has sequence 1.

**Note:** an earlier value computed for each dose is available to the
selection that dose reads, bare or qualified by its input identifier.

## Specification variants

- `spec_filter_qualified.yaml` and `spec_filter_bare.yaml` compare the
  dose's computed last day to both band bounds in a correlated filter.
- `spec_between_qualified.yaml` and `spec_between_bare.yaml` use the same
  computed last day as the range value.
- `spec_filter_expression.yaml` repeats the last-day arithmetic.
- `spec_self_join.yaml` compares another dose's start to the reading dose's
  computed last day, while excluding the reading dose and later doses.
