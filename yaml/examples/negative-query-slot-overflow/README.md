# ADaM ADAE: reject an event belonging to more queries than it has places

This example uses collected adverse events and the study's query dictionary to
attempt one row per adverse event:

- `AETERM` is the term the site reported and `AEDECOD` the dictionary term it
  was coded to;
- `SMQ01NAM` and `SMQ01CD` name the first standardized query the event belongs
  to and its dictionary identifier, and `SMQ02NAM` and `SMQ02CD` say the same
  for the second. Each pair is empty when the event's term is not in that
  query.

The dictionary gives every query a place to be reported in, and one place
holds one query. A term that two queries in the same place both claim has no
answer: choosing either would drop the other, and which one was dropped would
depend on the order the dictionary happened to be stored in. The run must
therefore fail and no artifact is accepted.

## How to fix

First decide how many queries the study reports. Every query the study
analyses needs its own place before the run, and the dictionary is where that
is decided, so give the second query a place of its own:

```csv
PREFIX,GRPNAME,GRPID,TERM
SMQ01,Severe Cutaneous Adverse Reactions,20000020,STEVENS-JOHNSON SYNDROME
SMQ02,Hypersensitivity,20000214,STEVENS-JOHNSON SYNDROME
```

Each new place also needs its own pair of variables, because how many an
artifact carries is fixed when the specification is written and not when the
dictionary is read. A study whose dictionary grows past what the specification
declares must be re-read against it rather than left to fill the places it
already has.

Do not resolve the clash by keeping whichever query is stored first. It
answers with a query the study did not choose, and it stops reporting the
other one at all.
