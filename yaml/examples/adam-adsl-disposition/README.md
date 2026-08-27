# ADaM ADSL disposition selection

This focused SDTM-to-ADaM probe answers one question: how is the final subject
disposition selected from DS?

`EOSDT` is the maximum date among disposition-event records. `EOSDECOD` and
`EOSREAS` sort matching DS rows by date and sequence and keep the last. The four
subjects cover completion, two same-day discontinuation records, no DS match,
and screen failure. Protocol milestones are excluded from the date reduction.

Expected `EOSDECOD.source.multiple_matches` and
`EOSREAS.source.multiple_matches` counts are both two. Ordered source selection
cannot currently use the `EOSDT` filter, so each subject's final DS row in this
positive fixture is disposition-relevant.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly four rows are
expected. A discontinued subject must have `DCSREAS`.
