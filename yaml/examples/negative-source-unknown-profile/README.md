# ADaM ADSL: reject a subject listing stored under an unnamed format

This example uses a collected subject listing to attempt one record per
subject:

- `SITEID` is the site the subject enrolled at.

The listing is stored under a name that says nothing about how to read it.
Its bytes happen to be separated by commas today, but nothing states that,
and a reader that decided by looking inside would read a file the study never
described -- differently in another season, or differently from the next
reader. The run must fail and no artifact is accepted.

## How to fix

Store the listing under the name of the format it is in, so that its reader
is chosen by what the study declares rather than by inspection:

```yaml
datasets:
  DM:
    path: input/dm.csv
```

A listing kept in some other format is converted before the study reads it,
and the converted file carries the name of what it now holds.
