# Reject an event claimed twice in one query place

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-query-slot-overflow.html)

**Goal:** show for each adverse event the reported term (`AETERM`),
the coded term (`AEDECOD`), and the two safety groupings
(`SMQ01NAM`, `SMQ01CD`, `SMQ02NAM`, `SMQ02CD`).

**Input:** collected adverse events carrying the reported term
(`AETERM`) and the coded term (`AEDECOD`), plus a query dictionary
carrying each coded term (`TERM`) under a grouping name (`GRPNAME`)
with code (`GRPID`) and place (`PREFIX`).

**Variables:**

- `SMQ01NAM` would be the name from the dictionary entry whose
  place is `SMQ01` and whose term matches the coded term; blank
  when no entry matches.
- `SMQ01CD` would be the code from that same `SMQ01` entry;
  missing when no entry matches.
- `SMQ02NAM` would be the name from the dictionary entry whose
  place is `SMQ02` and whose term matches the coded term; blank
  when no entry matches.
- `SMQ02CD` would be the code from that same `SMQ02` entry;
  missing when no entry matches.

A coded term claimed by two dictionary entries in the same place
has no single answer: choosing either entry would drop the other,
and which one was dropped would depend on the order the dictionary
happened to be stored in. The run is rejected with no artifact
accepted.

**Note:** one place holds one grouping, so a term claimed twice in
the same place cannot be reported without choosing.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

First decide how many queries the study reports. Every query the study
analyses needs a place of its own, so move the second query to the next free
place:

```csv
PREFIX,GRPNAME,GRPID,TERM
SMQ01,Severe Cutaneous Adverse Reactions,20000020,STEVENS-JOHNSON SYNDROME
SMQ03,Hypersensitivity,20000214,STEVENS-JOHNSON SYNDROME
```

A new place needs its own lookup and its own `SMQ03NAM` and `SMQ03CD`,
because how many groupings an artifact carries is fixed when the study rules
are written and not when the dictionary is read. Do not resolve the clash by
keeping whichever query is stored first: it answers with a query the study did
not choose, and it stops reporting the other one at all.
