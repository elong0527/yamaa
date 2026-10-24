# Reject Unknown Case

**Goal:** derive the site name (`SITE`) for analysis use with only its
first letter capitalized.

**Input:** collected demographics carrying the site name (`SITENM`),
recorded in capitals.

**Variables:**

- `SITE` would be the site name with its first letter capitalized and
  every later letter in lower case.

The requested case, `capitalize`, is none of the five cases a site name
can be put in: upper, lower, sentence, title, and proper. The request is
rejected before any data is read, and no artifact is accepted.

## How to fix

Decide which case the analysis value takes, then name it. A capital first
letter followed by lower case is sentence case:

```yaml
str_case:
  source: DM.SITENM
  to: sentence
```

A value such as `BOSTON GENERAL` then becomes `Boston general`. Writing
`to: title` instead capitalizes each word, giving `Boston General`.
