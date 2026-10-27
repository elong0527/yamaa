# Trial Arms with a Sponsor-Added Epoch

**Goal:** build the Trial Arms dataset, one record per planned element of
each arm, carrying `ARM`, `ETCD`, `ELEMENT` and `EPOCH`, and submit it as a
Dataset-JSON file with its data-definition document.

**Input:** the protocol's planned design, one row per element of each arm in
its planned order.

**Variables:**

- `ARMCD` and `ARM` are the planned arm's code and description.
- `TAETORD` is the element's position within its arm, starting at 1.
- `ETCD` and `ELEMENT` are the element's code and description.
- `EPOCH` is the trial period the element belongs to: `SCREENING` and
  `TREATMENT` from the published epoch terminology, and `EXTENSION`, the
  open-label extension period, which the sponsor added to that list.

**Note:** the data-definition document is prepared for a regulatory
submission, so every published epoch term carries its NCI code. The
sponsor's `EXTENSION` term has no NCI code: the document marks it as a sponsor
addition to the published list instead.
