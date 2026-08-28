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
`AESEVN` maps the term to 1, 2, 3, and `NEGSEVN` multiplies by `-1` so that
`row_number.order_by`, which is ascending with no direction option, puts the
most severe record first.

This is the case `../adam-adlb-closest-visit` predicted had no workaround. It
does have one, but only because severity can be given a numeric proxy. The
proxy is what makes the descending sort possible; a categorical column with no
meaningful numeric mapping still could not be ordered by preference.

`TEORD` ranks ineligible records last, the same eligibility-sort technique
`../adam-adae-occurrence-flags` records, because `row_number` cannot filter.
Two records in one partition are ineligible only if both have missing
`NEGSEVN`, which would require comparing missing values during ordering. This
fixture is built so that no partition holds more than one ineligible record.
That is a property of the data, not a guarantee of the specification, and it is
the reason portable missing-value ordering needs a rule.

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

## Status and named gaps

This fixture is a **probe**. It passes, and it names three gaps.

1. **Categorical ordering needs a numeric proxy.** A controlled vocabulary with
   a clinical order has none in the schema, so every specification restates the
   order as a `mapping` and then negates it.
2. **Only `row_number` exists.** Without `rank` and `dense_rank`, ties can be
   broken but not preserved, and a flag cannot cover a tied set.
3. **Ordering across missing values is undefined.** `TEORD` keeps ineligible
   records out of contention, but it does not define how two of them compare.

`AESEVN` is a real ADaM variable. `NEGSEVN`, `TEORD`, and `AWSRNK` are not, and
are emitted only because named intermediates are unsupported.

## Diagnostics and verifications

Expected `AESEVN.mapping.missing` count is one, for the event with no collected
severity. No `unmapped` handler is declared, so a severity outside the
dictionary would fail rather than pass silently; the `allowed_values` check on
`AESEV` states the same constraint at the column level.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly eight
rows are expected. The numeric severity and its negation must be present or
absent together, and a flagged record must be treatment-emergent and graded.
