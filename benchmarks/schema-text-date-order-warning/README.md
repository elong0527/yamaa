# Report Date Text Compared as Text

**Goal:** derive, for each PK sample, the start of the latest dose at or
before it (`LDOSDTC`) by comparing the collected date text as text, and
report where text order, not time, decided between two date texts.

**Input:** one `spec.yaml` declaring two datasets. `PC` carries one row per
sample: subject, sequence number and the collection date and time as
collected ISO 8601 text. `EX` carries the doses: subject, sequence number and
the start date and time as collected ISO 8601 text, some to the second and
one with no time.

**Variables:**

- `USUBJID`, `PCSEQ` and `PCDTC` are the sample's subject, sequence number
  and collection date and time, as collected.
- `LDOSDTC` is the start text of the subject's dose whose text is the
  greatest among the doses whose text is not greater than the sample's.
  Text order reads a date or time recorded to fewer fields as before one
  recorded to more that begins with it, so it is the text, not the time,
  that decides: subject S1's first sample, taken at `2025-03-11T08:00`,
  takes the previous day's dose because the dose at `2025-03-11T08:00:00`
  sorts after it, and subject S2's sample at `2025-03-11T07:30` takes the
  dose of that day, which has no time.

**Lookups:** `LAST_DOSE` names each sample's latest dose at or before it,
matched on the subject, narrowed by its filter and chosen by its order.

The run report (`adpc-checks.csv`) has one row per item the run handled, each
naming the dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec
states it (`SPEC_PATH`) and the item's name (`NAME`): the run and its outcome
(`OUTCOME`), the input datasets read, the samples built, the two counts of
the dose read, each naming the column it feeds (`TARGET`), and one row per
output column. Two warnings follow, one where the lookup's filter compares
the dose and sample texts and one where its order compares two doses. Each
has no label (`ID`) or column (`TARGET`), names the rule (`RULE`), its
`SEVERITY` and the condition (`CONDITION`), and counts its place as one unit
(`COUNT`, `TOTAL`, `KEYS`); its `DETAILS` name the least pair of texts that
text order decided there, the shorter first.

**Note:** the warnings change no value and fail nothing. Reading the sample
time as a `datetime` and completing each dose start with `impute`, choosing a
time for a dose recorded without one, compares times instead and gives no
warning.
