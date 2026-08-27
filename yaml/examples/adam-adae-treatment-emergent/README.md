# ADaM ADAE treatment-emergent classification

This focused SDTM-to-ADaM probe answers one question: is an adverse event start
date inside the subject's treatment interval?

## Rule and record grain

AE is the base, so each source event produces one ADAE row. ADSL contributes
`TRTSDT`, `TRTEDT`, and `TRTA` by `STUDYID` and `USUBJID`. The fixture's
sponsor-defined rule sets `TRTEMFL = Y` when `ASTDT` is within the inclusive
`[TRTSDT, TRTEDT]` interval.

The six events cover the day before treatment, both interval boundaries, the
day after treatment, an event strictly inside placebo treatment, and a subject
with no matching ADSL row. The unmatched subject remains in the output with
missing treatment values and no flag, demonstrating R003 left-join behavior.

## Contract

Rows remain in AE source order. The exact key is `[STUDYID, USUBJID, AESEQ]`,
and exactly six rows are expected. Dataset verifications require both treatment
dates or neither and require every flagged event to lie inside the interval.

This is a **probe** because date conversion and SQL comparison remain draft
under R004 and R005. The treatment-emergence definition is local to this
fixture, not a universal ADaM rule. No handler path is declared.
