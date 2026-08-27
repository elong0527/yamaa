# ADaM ADAE string normalization and handlers

This focused probe answers one question: how are case variation, missing text,
and malformed sponsor identifiers handled without host-language code?

`AETERMLO` lowercases reported terms. `AERELLC` lowercases relationship text
and substitutes `not reported` for a missing value; `AREL` maps that normalized
text to the analysis vocabulary. `AEREFNUM` extracts the numeric component of
an `AE-nnn` identifier, using `0` for missing input and `-1` for a non-matching
value before conversion to `int`.

The four rows contain upper/title-case text, one missing identifier, one
malformed identifier, and one missing relationship. Expected handler counts
are:

- `AEREFNUM.str_extract.missing`: 1;
- `AEREFNUM.str_extract.no_match`: 1;
- `AERELLC.str_lower.missing`: 1.

Rows remain in AE source order. The exact key is `[STUDYID, USUBJID, AESEQ]`,
and exactly four rows are expected. All output variables are eight characters
or fewer. This fixture covers only string operations and their local handlers.
