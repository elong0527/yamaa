# Uncollected codes stay missing; outside-codelist codes are labelled

**Goal:** pin how a dictionary lookup tells a missing code from a collected
code outside the dictionary.

**Input:** `spec.yaml` decodes coded sex from `DM` twice: once with a
separate named result for codes outside the dictionary, once with one named
result for both events. `DM` carries listed codes, a missing code, and a
code outside the dictionary.

**Note:** a missing code means "not collected"; a collected code outside the
dictionary means "outside the codelist" and is usually a data-quality
finding. The `SEXC` column keeps the missing code missing and reads the
outside code as `Outside codelist`. The `SEXC_SINGLE` column names the same
result, `Unknown`, for both events.
