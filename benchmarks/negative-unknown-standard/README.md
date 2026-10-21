# Reject a Device Dataset Whose Implementation Guide the Study Omits

**Goal:** build the device identifiers dataset, carrying `SPDEVID`,
`DIPARMCD`, `DIPARM` and `DIVAL`, and submit it with its data-definition
document.

**Input:** the identifying attributes of each study device as its
manufacturer supplies them, one row per attribute.

**Variables:**

- `SPDEVID` is the sponsor's identifier for one device.
- `DIPARMCD` and `DIPARM` name the identifying attribute: `MODEL` for the
  model number and `SERIAL` for the serial number.
- `DIVAL` is that attribute's value as the manufacturer printed it on the
  device.

**Note:** the device identifiers follow the medical device supplement to the
tabulation guide, but the study cites only the general tabulation guide for
its tabulation datasets. The document cannot point the dataset at a guide it
does not cite, so the submission stops before any file is written; the
dataset itself still builds.

## How to fix

Cite the medical device supplement beside the general guide in the
tabulation section, which the dataset then names:

```yaml
sdtm:
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}
    - {name: SDTMIG-MD, version: "1.1", status: Final}
```

If the dataset follows the general guide instead, remove its
`submission.standard`, and the dataset takes the section's first guide.
