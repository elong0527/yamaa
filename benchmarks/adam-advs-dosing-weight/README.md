# Weight Used for Dosing

**Goal:** retain the weight used for dosing and calculate body surface area
(`BSA`) at each treatment cycle.

**Input:** SDTM vital-signs weights, with visit dates and sequence numbers,
plus subject-level baseline height in centimetres.

**Variables:**

- `ADT` is the collection date, `AVISITN` the cycle number, and `AVAL` the
  collected weight in kilograms.
- `ANCHORWT` starts with the first observed weight. A change of at least
  10%, upwards or downwards from the retained weight, replaces it with the
  current weight. Smaller changes and missing measurements keep the prior
  retained weight. The replacement applies on the triggering cycle too.
- `BSA` is the square root of retained weight times baseline height divided
  by 3600, in square metres. Values are displayed to two decimal places.

The published cycle pattern follows Table 3 of Liu et al.,
[PharmaSUG 2020 DS-235][1].
Additional invented measurements exercise repeated exact-boundary resets,
missing weight and independent subjects.

[1]: https://pharmasug.org/proceedings/2020/DS/PharmaSUG-2020-DS-235.pdf
