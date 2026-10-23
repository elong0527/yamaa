# Report Repeated Exposure Records and Read the Latest

**Goal:** derive each subject's date of last exposure (`TRTEDT`) from the
exposure records, reporting the subjects with more than one record instead
of stopping the run.

**Input:** one `spec.yaml` declaring two datasets. `DM` carries one row per
subject. `EX` carries exposure records: subject, sequence number, start date
and end date.

**Variables:**

- `TRTEDT` is the end date of the subject's exposure record that starts
  last, the later sequence number breaking a tie; a subject with one record
  takes that record's end date. It is blank when the subject has no
  exposure record or the selected record has no end date.

**Lookups:** `LAST_EX` names each subject's latest exposure record. The
subject is expected to appear once in the exposure records; one that
appears more often is reported, and the latest of its records is read.

The run report (`adsl-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and the item's name (`NAME`, identifying an input,
row driver, handler, column, or check): the run and its outcome
(`OUTCOME`), the input datasets read, the subjects built, then the two counts
of the exposure read: the subjects for which the latest record was chosen
among several, and those for which no record was found. Each count names
the column it feeds (`TARGET`) and the rule its answer stands in for
(`RULE`). One row per output column follows, with its missing values and
longest text, then the one check, which carries its label (`ID`), its
`RULE` and `SEVERITY`, the condition it found (`CONDITION`), and how many
subjects repeat out of how many subjects the exposure records hold (`COUNT`,
`TOTAL`). Its `KEYS` list each repeated subject once, in the order its first
exposure record appears, and its `DETAILS` name the subject columns, the
exposure dataset and the lookup.

**Note:** the repeated subjects are reported before any row is built and
change nothing that is read: every subject still takes its latest record.
Without the order and the choice of the last record, a repeated subject
would have no record to read, so the check could not warn and would have to
stop the run.
