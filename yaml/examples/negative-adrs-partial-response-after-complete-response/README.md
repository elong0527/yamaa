# ADaM ADRS: reject a partial response recorded after a complete response

This example uses a series of collected tumour assessments to prepare one
record per assessment:

- `ADT` is the assessment date and `AVALC` the response recorded at it.

A subject whose disease has completely responded has no measurable disease
left to respond partially, so a partial response directly after a complete one
is a fault in the collected data rather than a course the disease can take.
Confirming a response reads these records in date order, so an assessment that
cannot be true would silently decide whether a subject counts as a responder.
The disagreement is reported against the assessment rather than corrected, and
no artifact is accepted from this input. The expected output records the
completed rows presented to that check.

An improvement in the other direction is ordinary and passes: a partial
response may later become complete.
