# Assign Each Subject's Region from Its Country

**Goal:** carry each subject's country in `COUNTRY` and give its
region in `REGION1`.

**Input:** demographics (`DM`) rows carrying the country of the
investigational site (`COUNTRY`) as the upper-case three-letter
ISO 3166 code SDTM collects, including countries outside the
regions listed below.

**Variables:**

- `COUNTRY` is the country from demographics, as collected; every
  subject has one.
- `REGION1` is the study's region for that country: `USA` and `CAN`
  give `North America`; `DEU`, `ESP`, `FRA` and `GBR` give `Europe`;
  and any other country gives `Rest of World`.

**Note:** a country outside the listed regions never stops the run;
it falls in `Rest of World`, so every subject has a region.
