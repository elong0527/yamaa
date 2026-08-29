# ADaM ADRS: confirm an objective response

This example uses an ordered series of overall tumour responses to derive one
record per assessment:

- `ADT` is the assessment date and `AVALC` its response;
- `CONFIRMED` is `Y` for progression, which needs no confirmation, and for a
  partial or complete response followed at least 28 days later by another
  partial or complete response. It is `N` when the next response is too early,
  is not a response, or does not exist.

Responses are read in date order within a subject, so a partial or complete
response at a subject's last assessment cannot be confirmed.
