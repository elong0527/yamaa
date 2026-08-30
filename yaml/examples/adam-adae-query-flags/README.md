# ADaM ADAE: record which queries a coded event belongs to

This example uses collected adverse events and the study's query dictionary to
derive one row per adverse event:

- `AETERM` is the term the site reported and `AEDECOD` the dictionary term it
  was coded to. An event still awaiting coding has no dictionary term;
- `SMQ01NAM`, `SMQ01CD`, and `SMQ01SC` name the first standardized query the
  event belongs to, its dictionary identifier, and whether the term is in that
  query's broad or narrow reading. `SMQ02NAM`, `SMQ02CD`, and `SMQ02SC` say
  the same for the second. A set is empty when the event's term is not in that
  query, and an event awaiting coding is in none of them;
- `CQ01NAM` names the customized query the sponsor defined for this study. A
  customized query is a list of terms rather than a dictionary grouping, so it
  carries neither an identifier nor a reading.

An event may belong to a standardized query and to the customized one at the
same time, and each is reported in its own place. Which query occupies which
place is decided by the dictionary rather than by the event, so two events
coded to the same term always report it in the same place.
