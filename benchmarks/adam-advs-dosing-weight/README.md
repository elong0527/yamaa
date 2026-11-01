# Weight Used for Dosing

**Goal:** retain the weight used for dosing and calculate body surface area
(`BSA`) at each treatment cycle.

**Input:** SDTM vital-signs weights, with visit dates and sequence numbers,
plus subject-level baseline height in centimetres.

**Variables:**

- `STUDYID` identifies the study.
- `USUBJID` identifies the subject within the study.
- `ASEQ` is the source measurement's sequence number.
- `AVISITN` is the treatment cycle number.
- `ADT` is the collection date, absent when no date was collected.
- `AVAL` is the collected weight in kilograms, absent for a missing
  measurement.
- `ANCHORWT` starts with the first observed weight. A change of at least
  10%, upwards or downwards from the retained weight, replaces it with the
  current weight. Smaller changes and missing measurements keep the prior
  retained weight. The replacement applies on the triggering cycle too.
  There is no retained weight until a weight has been observed.
- `BSA` is the square root of retained weight times baseline height divided
  by 3600, in square metres, absent when retained weight or baseline height
  is unavailable. Values are displayed to two decimal places.

The published cycle pattern follows Table 3 of Liu et al.,
[PharmaSUG 2020 DS-235][1].
Additional invented measurements exercise repeated exact-boundary resets,
missing weight and independent subjects.

[1]: https://pharmasug.org/proceedings/2020/DS/PharmaSUG-2020-DS-235.pdf
