# Reject Invalid Regex in str_replace

**Goal:** keep only the digits of the sponsor-defined identifier of each
adverse event.

**Input:** collected adverse events with one record for each event, carrying
the sponsor-defined identifier.

**Variables:**

- `AESPDIG` would hold the digits of the identifier in their original order,
  with every other character removed, and would be blank when the identifier
  is blank.

The pattern `[^0-9` is not a valid portable regex: the `[` opens a character
class that never closes. The request is rejected before any data is read, and
no artifact is accepted.

## How to fix

Decide which characters to remove. To keep only the digits, as the goal
states, close the class:

```yaml
str_replace:
  source: AE.AESPID
  pattern: '[^0-9]'
  replacement: ""
```
