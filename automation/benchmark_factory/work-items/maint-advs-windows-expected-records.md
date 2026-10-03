# Work item: advs-windows-expected-records

## Work item

**Type:** enhance. **Target:** `benchmarks/adam-advs-windows`
(reviewed). **Change:** add derived "expected" records for planned
analysis windows (SCREENING, BASELINE, WEEK 2, WEEK 4) in which a
subject/parameter has no collected record with a study day, following
the expected-records pattern (admiral `derive_expected_records()`).
Expected records carry the window (AVISIT/AVISITN), the planned visit
name/number, and a continued VSSEQ, but no date, day, value, or
analysis flag. POST-TREATMENT (open-ended, day 43+) never gets expected
records. Nothing else changes.

## What to measure

The expected-record pattern for analysis windows: a planned visit with
no measurement still appears in the analysis dataset so downstream
summaries see a complete visit spine. For each subject, parameter, and
planned window with no collected record whose study day falls in that
window, the benchmark derives exactly one expected record. The window
assignment rule itself (study-day cut) is unchanged; the new behavior
is the per-window coverage check plus the shape of the added records:
ADT/ADY/AVAL missing, ANL01FL blank, VSSEQ continuing the subject's
collected sequence. A collected record with a missing study day does
not satisfy a window (it belongs to no window, as before).

## Grounding material

- The existing benchmark itself: `benchmarks/adam-advs-windows`
  (spec, fixtures, golden, README, prompt). All 14 existing golden
  rows stay byte-identical on all existing columns.
- `references.md`: "The Phantom of the ADaM: adding missing records
  to BDS datasets" (Drach, PharmaSUG 2023) -- phantom derived records
  with missing AVAL/AVALC for missed visits, simple-to-complex
  scenario ladder.
- `references.md`: admiral `derive_expected_records()` -- the
  reference implementation of the expected-record pattern.
- Curated fixture additions (5 rows: 1 expected + 1 new subject with
  1 collected + 3 expected):
  - Subject 202, expected SCREENING (VSSEQ 7): exercises a single
    missing early window; window assigned without a study day.
  - Subject 203, collected BASELINE day 1 AVAL 121 (VSSEQ 1):
    exercises ANL01FL on a lone collected record.
  - Subject 203, expected SCREENING/WEEK 2/WEEK 4 (VSSEQ 2/3/4):
    exercises multiple expected records for one subject, continued
    sequencing, and that expected records never take ANL01FL.
  - No expected POST-TREATMENT record anywhere: exercises the
    open-ended window exclusion.

## Difficulty target

1. Expected records get their window from the planned visit, not from
   a study day -- the solver must not route them through the day-cut.
2. A collected record with a missing study day (202, VSSEQ 3) does
   not count as covering its collected visit's window.
3. Expected records never take ANL01FL, even when they are the only
   record in their window.
4. VSSEQ continues per subject (no collisions, no reuse).

## Out of scope

- No change to any existing column, value, or verification; the
  window-cut rule, AVISITN mapping, and ANL01FL tie-break are
  untouched.
- No DTYPE column (the pattern is identified by missing
  date/day/value, as in the phantom-records paper).
- No expected records for POST-TREATMENT.
- No new parameters; one new subject only.

## Acceptance sketch

- spec.yaml gains row templates (collected + 4 expected-visit
  templates) and per-window coverage intermediates; output.columns
  unchanged; row_count updated 14 -> 19; the
  baseline-window-is-study-day-one verification admits expected
  records; new verifications pin expected-record shape.
- input gains 1 row (subject 203 collected BASELINE); golden gains 5
  rows; every pre-existing golden value byte-identical (git diff).
- README documents the expected records; lifecycle badge reset to
  draft; Harbor prompt (full.md, conventions.md within 20 lines)
  updated and brief.md regenerated.
