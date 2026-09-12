# ADaM ADRS: combine efficacy, safety, and discontinuation into one response

This example uses a pre-derived ADRS slice with ADSL and a `yamaa`
specification to derive one row per subject:

- `PCHG` is the percentage change in the efficacy measure, `SAEFL` says whether
  the subject had a serious adverse event, and `DCSREAS` gives their reason for
  discontinuing, if any;
- `AVALC` is the responder value, decided in a fixed order. A subject with a
  serious adverse event or any discontinuation reason is a non-responder
  whatever their efficacy value. Otherwise a subject with no efficacy value is
  not evaluable, one whose change is at least the response threshold is a
  responder, and everyone else is a non-responder. `AVAL` is its numeric
  companion;
- `ARSN` records which of those four rules applied, because three of them
  produce the same non-responder value and the value alone does not say why.

The order matters and is part of the definition: a subject who meets the
efficacy threshold but had a serious adverse event is a non-responder, and a
subject with no efficacy value who discontinued is a non-responder rather than
not evaluable.
