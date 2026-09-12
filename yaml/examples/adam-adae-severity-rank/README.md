# ADaM ADAE: rank a subject's events by severity

This example uses collected adverse events to derive one record per event:

- `ASEV` is the reported severity of the event, and is empty when severity was
  not reported;
- `ASEVN` is that severity as a number, largest for the worst;
- `SEVRANK` places the event among the subject's events, worst first. Events of
  equal severity take the same place and the places they would otherwise have
  filled are left out, so a place of one means the subject reported nothing
  worse;
- `SEVLVL` numbers the severities the subject reported, worst first and each
  counted once, so its largest value is how many different severities they
  reported.

Events whose severity was not reported come after every reported severity and
share one place with each other, because nothing tells them apart. A subject
whose events are all of one severity has that severity in first place on every
record.
