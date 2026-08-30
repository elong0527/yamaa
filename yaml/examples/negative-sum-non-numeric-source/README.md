# ADaM ADAE: reject a severity burden totalled from severity words

This example uses collected adverse events to attempt one record per event:

- `ASEV` is the reported severity of the event;
- `SEVTOT` is meant to be a subject's total severity burden across the events
  reported for them.

Severity is recorded as a word, and words have no total. Ordering the words and
totalling their positions is a real rule, but it is a different one, and the
specification never states the numbers it would use. Choosing them here would
put the study's severity scale outside the specification, so the run must fail
and no artifact is accepted.
