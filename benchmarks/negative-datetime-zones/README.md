# Reject Mixed Zones

**Goal:** carry each adverse event (AE) term into `AETERM` and its
start moment into `ASTDTM`.

**Input:** collected adverse event records carrying term (`AETERM`)
and start text (`AESTDTC`).

**Variables:**

- `ASTDTM` would contain the start moment copied from `AESTDTC`, a
  site-clock reading with no clock offset. A start that carries an
  offset, such as `+02:00`, is not the same kind of value, so the
  run stops while converting values and no artifact is accepted.

## How to fix

Decide whether the study collects an offset at all. If the site clock is what
was recorded, correct the collected value so every start reads the same way:

```
YAMAA-01,YAMAA-01-101,2,HEADACHE,2025-03-04T09:00:00
```

If the offset is real data, collect it as a field of its own and keep
`AESTDTC` to the site clock. `ASTDTM` then holds the moment and a second text
column holds the offset, where a later analysis can read it.

To inspect the collected characters without failing, carry them in an
intermediate text column that is not `ASTDTM`:

```yaml
- name: AESTDTC_RAW
  type: str
  label: Collected Start Text
  derivation: AE.AESTDTC
```

The text keeps the collected characters but does not order
chronologically across offsets, and it is not an analysis variable:
`ASTDTM` stays a moment.
