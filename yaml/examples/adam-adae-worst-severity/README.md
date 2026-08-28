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

The mapping is still required. Severity has no order in the schema, so the
vocabulary needs a numeric proxy before anything can sort it. What is no longer
required is a second column: the proxy used to be negated into `NEGSEVN` so
that an ascending-only `order_by` would put the most severe record first.

`TEORD` ranks ineligible records last, the same eligibility-sort technique
`../adam-adae-occurrence-flags` records, because `row_number` cannot filter.
Two records in one partition are ineligible only if both have a missing
`AESEVN`, and R007 now defines how they compare: `nulls` defaults to `last`, so
they sort after every graded record and then break their tie on `ASTDT` and
`AESEQ`. This fixture still contains no such partition, but that is no longer
load-bearing. It used to be a property of the data rather than a guarantee of
the specification.

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

This fixture is a **probe**. It passes, and one of the three gaps it named
remains.

1. **Categorical ordering needs a numeric proxy.** A controlled vocabulary with
   a clinical order has none in the schema, so every specification restates the
   order as a `mapping`. Declaring `direction: desc` removed the negation but
   not the mapping: the order still lives in a dictionary rather than in the
   vocabulary.
2. **Closed: `order_by` had no direction.** The order term declares it.
3. **Closed: ordering across missing values was undefined.** `nulls` declares
   the placement, and R007 fixes the default at `last` under both directions
   rather than inheriting an engine's convention.

Still open beside these: without `rank` and `dense_rank`, ties can be broken
but not preserved, so a flag cannot cover a tied set. Subject
`CATH-UCSD-0002` has two events tied on severity and date, and only one is
flagged.

`AESEVN` is a real ADaM variable and stays in the output. `TEORD` and `AWSRNK`
are not, and declare `output: false`.

## Diagnostics and verifications

Expected `AESEVN.mapping.missing` count is one, for the event with no collected
severity. No `unmapped` handler is declared, so a severity outside the
dictionary would fail rather than pass silently; the `allowed_values` check on
`AESEV` states the same constraint at the column level.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly eight
rows are expected. The numeric severity and its negation must be present or
absent together, and a flagged record must be treatment-emergent and graded.
