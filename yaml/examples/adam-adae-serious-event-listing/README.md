# ADaM ADAE: list the serious adverse events

This example uses sample AE records and produces one row per serious event:

- `AETERM` is the term the investigator reported for the event;
- `AESER` is `Y` on every row, because a non-serious event is not listed.

A study whose events are all non-serious produces this listing with no rows in
it. The result still carries its variable names, so a reader can tell an empty
listing apart from a listing that was never produced, and a later step reads it
without treating it as a special case.
