# SDTM AE: code reported terms against a medical dictionary

This example uses collected adverse events with a MedDRA extract and a `yamaa`
specification to derive one record per event:

- `AETERM` is the term the site reported, kept exactly as written;
- `AEDECOD` is the dictionary's preferred term for it, and `AEBODSYS` the body
  system that term belongs to. Both are found by matching the reported term
  against the dictionary's lowest-level terms.

The match is exact, so a term the dictionary does not contain, and one that
differs only in case from a term it does contain, both read `NOT CODED`, as
does an event whose term was never reported. Leaving them as `NOT CODED` rather
than empty makes an uncoded event visible as a data-management question instead
of an absent value.

The dictionary is an ordinary input, and the study records which version was
used, since the same term can code differently between releases.
