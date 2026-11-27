# Check Only the Records a Dataset Part Keeps

**Goal:** publish the category B findings, `obs_b.csv`, with the subject
(`SUBJ`), the sequence number (`SEQ`) and the value (`VAL`), and check the
keys, the values and the record count over the published records alone.

**Input:** `input/findings.csv` holds five findings. The two category A
findings are faulty: one repeats subject `S1`'s sequence number 1 and misses
its value, and one misses its sequence number. Two spec files:

- `spec_part.yaml` keeps the category B findings and writes its run report to
  `obs_b-checks.csv`;
- `spec_empty.yaml` names `spec_part.yaml` as its parent and keeps the
  category Z findings, of which there are none, writing `obs_z.csv` and
  `obs_z-checks.csv`.

**Variables:**

- `SUBJ`, `SEQ` and `VAL` carry the subject, the sequence number and the
  value from the input record.

Each run report has one row per item the run handled, each naming the
dataset (`ARTIFACT`), the kind of item (`KIND`), where the spec states it
(`SPEC_PATH`) and the item's name (`NAME`): the run and its outcome
(`OUTCOME`), the findings read and built, each published variable with its
missing values and longest text (`DETAILS`), then one row per check. A check
row carries no label (`ID`), the variable it covers (`TARGET`), the rule it
applies (`RULE`), its `SEVERITY`, and how many records failed out of how many
it examined (`COUNT`, `TOTAL`); every check holds, so `CONDITION` and `KEYS`
are empty.

**Note:** the faulty findings are built and then left out, so the subject and
sequence number identify each published record, no published value is
missing, and the count of at most three records holds. The run report counts
the three published records (zero for the empty part) for every variable and
check, while its row-construction line still counts the five findings read
and built.
