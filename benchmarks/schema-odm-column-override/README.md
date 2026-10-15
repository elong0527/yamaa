# Replace Collected Sex with a Reviewed Unknown Value

**Goal:** record `SEX` as unknown for each subject after review, even when the
extract contains a collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, including a repeated sex answer at another visit.

**Variables:**

- `SEX` is `U` for each subject. The reviewed value replaces the collected
  answer, including when that answer appears more than once.
