# ADaM ADSL population flags

This fixture answers one question: how are safety and intent-to-treat flags
derived from existing ADSL state?

`SAFFL` is `Y` when `TRTSDT` is present. `ITTFL` is `Y` when `ARMCD` is present.
The four rows cover every combination used by the original combined example:
treated and randomized, randomized but untreated, and neither.

The input is a pre-derived ADSL slice so treatment selection does not obscure
the flag rules. Rows remain in input order; the key is `[STUDYID, USUBJID]`;
exactly four rows are expected. Named implications verify the evidence required
for each positive flag. No handler path is declared.
