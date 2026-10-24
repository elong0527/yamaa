# Free-Text Start Dates Read as ISO 8601 Dates

**Goal:** one row per concomitant medication with its analysis start as
ISO 8601 text (`ASTDTC`) and as a date (`ASTDT`), read from a start the
sites recorded as free text in several spellings.

**Input:** concomitant medication records, each identified by its sequence
number (`CMSEQ`), carrying the reported name (`CMTRT`) and the collected
start (`CMSTDAT`) as the site wrote it.

**Variables:**

- `ASTDTC` is the start as ISO 8601 text at the precision it was collected:
  `2001-03-04` for a full date and `2001-03` for a month alone. It is empty
  when no start was collected.
- `ASTDT` is the analysis start date: the collected date, or the first day
  of the month when only a month was collected. It is empty when no start
  was collected.

**Note:** five spellings of the start are accepted: an ISO 8601 date such as
`2001-03-04`; a two-digit day, three-letter month and four-digit year such
as `04MAR2001`, the month in any letter case; a slashed date such as
`03/04/2001`, which reads month first; eight digits such as `20010304`; and
a three-letter month and year such as `MAR2001`. A start written in any
other spelling stops the run rather than being guessed.
