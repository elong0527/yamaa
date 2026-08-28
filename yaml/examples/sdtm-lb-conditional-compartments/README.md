# SDTM LB conditional compartments

This fixture answers one question: can a structurally inapplicable compartment
be told apart from a sample that was applicable but not collected?

## Rule and record grain

Skin biopsies are collected from two compartments. Subjects with atopic
dermatitis or psoriasis have both a lesional and a non-lesional sample.
Non-atopic subjects have no lesion, so the lesional compartment does not exist
for them; it is not a missing result.

Two row templates express that. The lesional template is filtered to applicable
cohorts, so a non-atopic subject produces no lesional record at all. The
non-lesional template is unfiltered. An applicable subject whose sample was not
analyzed still produces a record, with a missing result and `LBSTAT` set to
`NOT DONE`.

The three subjects produce five records: two compartments for the atopic and
psoriasis subjects, one for the non-atopic subject. Subject `CATH-UCSD-0002`
shows the applicable-but-uncollected case; subject `CATH-UCSD-0003` shows the
structurally absent one.

## Absence and exclusion look identical

The distinction works, and it works because the specification author knew which
filter to write. Nothing in the schema records that the lesional compartment is
conditional on cohort. A reader of the output sees that `CATH-UCSD-0003` has
one record and must infer why.

`../sdtm-lb-multiform` suppressed both a structurally absent item and an
explicitly blank one with the same row filter and recorded the ambiguity as an
open question. This fixture resolves it for the author, not for the schema: the
two cases now produce different output, but only because two different
mechanisms were chosen by hand.

## Three gaps this fixture names

**Applicability is not declarable.** A conditional compartment is expressed as
a row filter. Nothing states that a parameter is expected for one population and
inapplicable for another, so no implementation can check the filter against the
protocol.

**Expected record counts cannot be asserted.** The fixture verifies that a
`NOT DONE` record has no result and that a collected result is not flagged. It
cannot verify that every atopic subject has exactly two records, because
verifications are row-wise over the completed output and no counting aggregate
exists.

**Row filters see only the row driver.** `row.filter` references `BX_RAW`
alone. A conditional compartment whose applicability lived in another dataset,
such as a diagnosis in DM, could not be filtered without first copying that
column onto the driver.

## Diagnostics and verifications

No handler path is declared. `LBSTRESN` converts a collected character result
to float and every present value converts cleanly.

Rows are appended in row-template order, so both lesional records precede the
non-lesional ones. `LBSEQ` is assigned within subject by `LBLOC` and
`LBTESTCD` after both templates are appended, which is why sequence one appears
on a lesional record for two subjects and on a non-lesional record for the
third.

The exact key is `[STUDYID, USUBJID, LBSEQ]`, and exactly five rows are
expected.
