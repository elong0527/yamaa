# Reject a source ordinal name collision

**Goal:** preserve collected values when original record positions are
requested for review.

**Input:** a listing with record labels and a collected position field.

**Expected:** the run is rejected when a generated position would replace a
collected field. The stored values are never overwritten or renamed.

## How to fix

Choose an ordinal name that is absent from the stored input fields, such as
`ordinal: OriginalPosition`, and reference `SOURCE.OriginalPosition` in the
consuming expressions. See
[REQ-1294](../../rules/storage/ingestion.md#req-1294).
