# ADaM ADPC: reject a sample time outside the day

This example uses collected pharmacokinetic samples to attempt one analysis row
per specimen:

- `PCTM` is meant to hold the local time at which the specimen was collected.

Hour 24 is not a time within a day. Midnight belongs to hour `00` of the next
calendar date, so accepting `24:00` would leave that date ambiguous and the run
must fail.

## How to fix

Confirm the collection date and record midnight as `00:00` on the following
calendar date:

```text
PILOT7,P7-202,1,00:00
```
