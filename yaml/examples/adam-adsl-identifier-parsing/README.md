# ADaM ADSL identifier parsing and fallback

This SDTM-to-ADaM fixture answers one question: how is a site parsed from
`USUBJID` with a collected `SITEID` fallback?

`SITEIDP` extracts the site from `CATH-<site>-<four digits>`. `SITEID`
coalesces the parsed value with `DM.SITEID`, and `SUBJREF` concatenates the
resolved site and subject identifier. The four rows contain two valid compound
identifiers, one malformed identifier, and one missing `SUBJID`.

Expected handler counts are one `SITEIDP.str_extract.no_match`, zero
`SITEIDP.str_extract.missing`, and one `SUBJREF.str_concat.missing`. Rows remain
in DM order; the key is `[STUDYID, USUBJID]`; exactly four rows are expected.
