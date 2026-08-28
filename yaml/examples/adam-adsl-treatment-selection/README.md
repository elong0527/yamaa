# ADaM ADSL treatment selection and duration

This focused SDTM-to-ADaM probe answers one question: how are a subject's
actual treatment and inclusive treatment interval selected from EX?

The two treated subjects each have two EX rows stored out of chronological
order. `TRT01RAW` selects the first treatment by date and sequence; `TRTSDT`
and `TRTEDT` reduce the qualifying start and end dates; and `TRTDURD` adds one
to the date difference. Placebo administrations have `EXDOSE = 0` but remain
real treatment records. Two subjects have no EX: one falls back to planned
treatment and one to `NOT TREATED`.

Expected `TRT01RAW.source.multiple_matches` count is two. The current language
cannot filter ordered `source.multiple_matches`, so every EX row in this
positive fixture is treatment-relevant. `TRT01RAW`, `TRT01SRC`, and `TRTDUR0`
declare `output: false` and stay out of the artifact; the
`treatment-period-completeness` verification still names `TRTDUR0`, which R005
permits for an internal column.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly four rows are
expected. Treatment dates and durations must be all present or all missing,
and placebo must imply `SAFFL = Y`.
