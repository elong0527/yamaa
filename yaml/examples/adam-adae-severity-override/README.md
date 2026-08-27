# ADaM ADAE approved severity override

This focused probe answers one question: does a final override run after normal
conversion and before a dependent variable is derived?

`ASEV` first uppercases the collected severity. The approved correction changes
subject `CATH-01-001`, event 2, from `MODERATE` to `SEVERE`. `ASEVN` then maps
the final `ASEV`, so the corrected row must contain both `SEVERE` and `3`.

The expected count for `ASEV.override[0]` is one. Rows remain in AE source
order, the exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly three rows are
expected. The named implication verifies that the correction is visible to the
dependent mapping. No other handler or business rule is included.
