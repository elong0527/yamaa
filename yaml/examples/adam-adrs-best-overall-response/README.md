# ADaM ADRS: select the best overall response

This example uses a subject's randomization date and the overall responses
prepared for them to derive one best-response record per subject:

- `RANDDT` is the randomization date the response window is measured from;
- `AVALC` is the best response the subject achieved and `AVAL` its rank, `1`
  for a complete response through `6` for an assessment that was not
  evaluable. A complete or partial response counts whenever it occurred;
  stable disease and neither-complete-nor-progressive disease count only from
  42 days after randomization, so a subject assessed as stable too early does
  not qualify on it. Progression is next, and an assessment qualifying for
  none of these leaves the subject not evaluable. A subject with no assessment
  at all has no best response;
- `ADT` is the date of the earliest assessment supporting that response, which
  for a subject who is not evaluable is the earliest assessment that was not
  progression.

The order is the definition, not a preference: a subject whose only stable
assessment came too early is progressive when a progression follows, and not
evaluable when none does. Both outcomes rest on the same assessment.
