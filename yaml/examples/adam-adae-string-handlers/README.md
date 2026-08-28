# ADaM ADAE string handlers

This fixture answers one question: can a missing string be told apart from a
present one that fails to match, and does the replacement value reach a
dependent column?

## Rule and handler paths

`AEREFNUM` extracts the numeric part of a sponsor identifier written
`AE-<three digits>`. The two failure conditions get different sentinels: an
absent `AESPID` becomes `0` and a present but malformed one becomes `-1`, so
the output records which condition occurred. `../adam-adsl-identifier-parsing`
declares the same two paths on `str_extract` and sends both to `null`, which
proves the handlers fire but discards the distinction; this fixture keeps it.

`AETERMLO` lowercases `AETERM`, whose values are collected in mixed case, and
declares no handler because the column is always collected. `AERELLC`
lowercases `AEREL` with `missing: not reported`, and `AREL` uppercases
`AERELLC` and verifies the result against the codelist. The handler literal is
therefore written in the intermediate column's own lowercase domain rather than
the codelist's, because the dependent column is what converts it: `not
reported` becomes `NOT REPORTED` only because `AREL` runs after the
substitution.

The four rows cover a matching identifier, an absent one, a malformed one, and
a second subject. `AEREFNUM` is declared `int` while both handler literals are
quoted strings, so each is substituted into the string result of `str_extract`
and converted under R011, which parses a leading `-`. The `range` verification
admits `-1` because the sentinel is a value the rule intends, not a defect.

## What the fixture does not settle

The sentinels are integers in the same domain as a real identifier number, so a
consumer must know that `0` and `-1` are not sponsor identifiers. Nothing in
the specification says so; the choice is legible only in the handler
declaration.

Row 3 also depends on gap 7 of `../README.md`: `AEREL` exists as a column and
its value is empty, so `str_lower.missing` fires on a missing value rather than
on an absent variable, which is the expression-handler reading of R008. No rule
says an empty CSV field is that missing value.

## Diagnostics and verifications

Expected handler counts are one `AEREFNUM.str_extract.missing`, one
`AEREFNUM.str_extract.no_match`, one `AERELLC.str_lower.missing`, and zero
`AETERMLO.str_lower.missing`. No override or conversion handler is declared, so
a malformed value that reaches conversion is fatal.

Rows remain in AE source order; the exact key is `[STUDYID, USUBJID, AESEQ]`,
and exactly four rows are expected.
