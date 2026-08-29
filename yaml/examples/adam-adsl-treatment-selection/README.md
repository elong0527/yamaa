# ADaM ADSL treatment selection and duration

This SDTM-to-ADaM fixture answers one question: how are a subject's actual
treatment and inclusive treatment interval selected from EX?

The two treated subjects each have two EX rows stored out of chronological
order. `TRT01RAW` selects the first treatment by date and sequence; `TRTSDT`
and `TRTEDT` reduce the qualifying start and end dates; and `TRTDURD` counts
both endpoints. Placebo administrations have `EXDOSE = 0` but remain
real treatment records. Two subjects have no EX: one falls back to planned
treatment and one to `NOT TREATED`.

Expected `TRT01RAW.source.multiple_matches` count is two. `TRT01RAW` declares
the same `filter` as `TRTSDT` and `TRTEDT`, so all three agree on which
exposure records qualify; every EX row in this positive fixture is
treatment-relevant, so the filter changes no value here and exists to keep the
three derivations from drifting apart. `TRTDURD` counts both endpoints with
`bounds: inclusive`, so the inclusive duration needs no intermediate.
`TRT01RAW` and `TRT01SRC` declare `output: false` and stay out of the
artifact.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly four rows are
expected. Treatment dates and durations must be all present or all missing,
and placebo must imply `SAFFL = Y`.
