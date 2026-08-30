# ADaM ADCE: flag the subject's worst-grade event

This example uses collected solicited events with a `yamaa` specification to
derive one row per event:

- `CESEQ` is the collected sequence number that identifies the event within
  its subject;
- `CETERM` is the reported event and `ASTDT` the date it started;
- `ASEV` is the collected severity and `ASEVN` its order: mild, moderate, and
  severe count one, two, and three. An event collected without a severity has
  neither;
- `ATOXGRN` is the severity restated as a numeric toxicity grade, so a
  moderate event reads grade 2;
- `AOCCFL` marks the subject's worst-graded event: the highest grade, the
  earliest start among events tied on grade, and the lower sequence number
  among events tied on both. An event without a grade can never be the worst.

Grading and flagging are kept separate so that the grade means the same thing
on every event, while the flag answers a question about the subject.
