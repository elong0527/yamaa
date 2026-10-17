# Reject Remote Parent

**Goal:** attempt to prepare subject records reusing shared
definitions named by `https://example.test/organization.yaml`.

**Input:** demographics records carrying the unique subject
identifier.

**Variables:**

The requested result names no variables. It would carry subject
records prepared from the shared definitions.

**Note:** the run is rejected before any data is read and no
artifact is accepted: a remote resource can change independently
and cannot provide a reproducible local build.

## How to fix

Review and store the parent file locally, then reference it with a relative or
absolute filesystem `parent` path. Do not use a URL or URI.
