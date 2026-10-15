# Reject Case Collision

**Goal:** build `SMOKEFL` from collected smoking status, matching
without regard to case.

**Input:** collected demographics carrying reported smoking status
(`SMOKSTAT`).

**Variables:**

- `SMOKEFL` would be the smoking flag taken from `SMOKSTAT`
  without regard to case: `Y` gives `Y`, `y` gives `Y`, and `N`
  gives `N`. The entries for `Y` and `y` collide on the key `Y`
  once case is ignored, so the run is rejected before any data is
  read and no artifact is accepted.

## How to fix

For case-insensitive matching, retain only one dictionary entry for each
folded key. Both `Y` and `y` then resolve through `Y`:

```yaml
mapping:
  source: DM.SMOKSTAT
  case_sensitive: false
  dict:
    Y: "Y"
    N: "N"
```

Alternatively, set case sensitivity on when differently cased values are
intentionally distinct and give each one an explicit meaning.
