# Bind Each Dataset to the Implementation Guide It Names

**Goal:** submit demographics and device identifiers with one
data-definition document that cites both implementation guides they follow,
each dataset pointing at its own.

**Input:** two spec files and the project that lists them:

- `spec_dm.yaml` builds demographics (DM) from the study's subjects and names
  no guide;
- `spec_di.yaml` builds device identifiers (DI) from the identifying
  attributes of each study device, one row per attribute as its
  manufacturer supplies them, and names the medical device supplement to
  the tabulation guide;
- `project.yaml` cites the general tabulation guide first and the medical
  device supplement second.

**Variables:**

- `SPDEVID` is the sponsor's identifier for one device.
- `DIPARMCD` and `DIPARM` name the identifying attribute: `MODEL` for the
  model number and `SERIAL` for the serial number.
- `DIVAL` is that attribute's value as the manufacturer printed it on the
  device.

**Note:** the data-definition document cites every guide the project lists,
in the project's order. Each dataset's definition points at the guide its
spec names, or at the first guide when it names none, so demographics
follows the general guide and the device identifiers follow the supplement.
