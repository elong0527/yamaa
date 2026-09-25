# Related Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-relrec-links.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** emit one related-records row per relationship
participation, carrying `IDVARVAL`, `RELTYPE`, and `RELID`.

**Input:** the finished AE and CM datasets: each record already
carries an assigned sequence number (AESEQ on AE, CMSEQ on CM) and
up to two link identifiers (AELNKID1/AELNKID2 on AE,
CMLNKID1/CMLNKID2 on CM). A second subject reuses link number 1,
showing that link numbers are per-subject: the USUBJID key keeps
the two relationships apart.

**Variables:**

- `IDVARVAL` is the sequence number of the related record as text;
  always present.
- `RELTYPE` is blank throughout, because each row points at one
  record rather than a whole dataset.
- `RELID` names the relationship the row takes part in; rows
  sharing a value are related to one another, and every row
  carries one.

**Note:** a record with no link identifier contributes no row,
while a record naming two link identifiers contributes one row per
identifier - only when the two identifiers differ. A record
repeating its link number in both fields produces two identical
rows, which fails the run on the duplicate key; carrying a third
relationship would need another link field on the collected
record.

**Negative case:** a link number carried by only one record is a
dangling relationship and fails the run. Adding the collected row
`CATH,CATH-UCSD-0001,4,COUGH,3,` to ae.csv (AELNKID1=3, no other
record carrying link 3) emits one RELREC row, and the
`each-relationship-has-two-or-more-rows` check then fails: the
group (STUDYID=CATH, USUBJID=CATH-UCSD-0001, RELID=3) holds 1 row,
below the minimum of 2. That is why the passing input above
carries no such row.

**Standard:** SDTM | **Domain:** RELREC
