# ADaM ADAE worst-severity selection

This focused probe answers one question: can the worst-severity record be
selected when the ordering is a controlled vocabulary rather than a number?

`../adam-adae-occurrence-flags` covers first-occurrence flags at subject, SOC,
and preferred-term levels. This fixture keeps only the preferred-term level and
changes the ordering criterion from time to severity, which is where the
language behaves differently.

## Rule and input boundary

The input is a small preclassified ADAE slice carrying `TRTEMFL`. Within a
subject and preferred term, the treatment-emergent event of greatest severity
is the worst-case record. Ties on severity are broken by earliest start date
and then by `AESEQ`, so the selection is total. `AWSEVFL` is sponsor-defined,
not a standard ADaM variable.

The eight records cover three severities of one term, a term with a single
event, an event that is not treatment-emergent, two events tied on both
severity and date, and an event whose severity was not collected.

## Severity has no order until it is given one

`AESEV` is `MILD`, `MODERATE`, `SEVERE`. Sorted as text that order is
`MILD`, `MODERATE`, `SEVERE` by coincidence of spelling, and `SEVERE` would sort
last rather than first in any case. The clinical order has to be introduced:
`AESEVN` maps the term to 1, 2, 3, and the order term declares
`{variable: AESEVN, direction: desc}`.

The mapping is the gap this fixture names. A controlled vocabulary with a
clinical order has none in the schema, so every specification restates that
order as a `mapping` dictionary rather than reading it from the vocabulary.

`AWSRNK` declares `filter: "TRTEMFL = 'Y' AND AESEVN IS NOT NULL"`, so an
ungraded or non-emergent event is never numbered and `AWSEVFL` tests the rank
alone. A preferred term whose events are all ineligible yields no rank at all
rather than a spurious rank of one.

## One record is flagged, not all tied records

Subject `CATH-UCSD-0002` has two `MODERATE` headaches on the same day. They tie
on severity, date, and everything except `AESEQ`, so `row_number` assigns 1 and
2 and exactly one is flagged.

That is correct for a flag that must identify a single record, and wrong for
sponsors who flag every record tied at the worst severity. Only `row_number` is
registered; `rank` and `dense_rank` are not, so "all records sharing the worst
severity" cannot be expressed and neither can any count of distinct severity
levels. The distinction is invisible in this fixture's output, which is why it
is stated here.

`AESEVN` is a real ADaM variable and stays in the output. `AWSRNK` is not, and
declares `output: false`.

## Diagnostics and verifications

Expected `AESEVN.mapping.missing` count is one, for the event with no collected
severity. No `unmapped` handler is declared, so a severity outside the
dictionary would fail rather than pass silently; the `allowed_values` check on
`AESEV` states the same constraint at the column level.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly eight
rows are expected. A flagged record must be treatment-emergent and graded.
