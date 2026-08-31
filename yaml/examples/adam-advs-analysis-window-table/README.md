# ADaM ADVS: assign analysis windows from the study's window table

This example uses a pre-derived ADVS slice and one study-wide analysis window
table shared by all parameters to derive one row per record:

- `ADT`, `ADY`, and `AVAL` are the record's analysis date, its study day, and
  the value measured, all carried through as given;
- `AVISIT` and `AVISITN` are the analysis window the record falls in, and its
  order. The table states each window's first and last study day, so a record
  belongs to the one window whose range contains its day, and a record with no
  study day belongs to none;
- `AWTARGET` is the target analysis day defined by the window table, and
  `AWTDIFF` is how far the record sits from it, negative before the target and
  positive after;
- `ANL01FL` marks the record that represents its subject and parameter in the
  window: the one closest to the target day, and the lower sequence number
  when two are equally close.

The window boundaries remain in the study table rather than being copied into
the specification. A record with no study day, or with a complete day outside
all declared windows, has no window, target, distance, or analysis flag. The
sample includes two records equally close to one target to exercise the lower
sequence-number decision.
