# ADaM ADSL disposition selection

This focused SDTM-to-ADaM probe answers one question: how is the final subject
disposition selected from DS?

`EOSDT` is the maximum date among disposition-event records. `EOSDECOD` and
`EOSREAS` sort matching DS rows by date and sequence and keep the last. The four
subjects cover completion, two same-day discontinuation records, no DS match,
and screen failure. Protocol milestones are excluded from the date reduction.

`EOSDT`, `EOSDECOD`, and `EOSREAS` now declare the same `filter`, so the date
reduction and the two ordered selections agree on which records are eligible by
construction rather than by coincidence. Expected
`EOSDECOD.source.multiple_matches` and `EOSREAS.source.multiple_matches` counts
are both one: only `CATH-UCSD-0002` has more than one disposition event once
protocol milestones are excluded. The selected values are unchanged, because
the milestone records never sorted last.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly four rows are
expected. A discontinued subject must have `DCSREAS`.
