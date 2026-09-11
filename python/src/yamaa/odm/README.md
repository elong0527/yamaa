# ODM to Parquet

This component streams supported CDISC ODM exports into one canonical Parquet
file with one row per clinical item. Pydantic validates each row boundary and
Polars performs all tabular construction, validation, and serialization.

The production path creates no CSV data or intermediate dataset. Each bounded
record batch becomes a Polars frame; an incremental Arrow encoder writes those
frames into one staged Parquet file, which Polars validates before atomic
publication. Arrow is used only for Parquet serialization. A CSV illustration,
when needed, must be exported downstream from the completed Parquet dataset.

```bash
PYTHONPATH=python/src uv run --with-requirements python/requirements-odm.txt \
  python -m yamaa.odm \
  input.xml output.parquet --profile strict

PYTHONPATH=python/src uv run --with-requirements python/requirements-odm.txt \
  python -m yamaa.odm \
  kn189_odm.tar.gz kn189.parquet --profile kn189
```

The `cart-t-openclinica` profile explicitly maps the seven observed site
clinical metadata references to the supplied main catalog. The `kn189` profile
selects `KN189_odm_dag/odm.xml` and projects the outer item group to form
context and the inner item group to item-group context. Original study and
metadata-version identifiers remain unchanged in output rows.

Focused tests:

```bash
PYTHONPATH=python/src uv run \
  --with-requirements python/requirements-odm-test.txt \
  python -m pytest python/tests/odm
```

The optional full-data tests use local, checksum-verified copies:

```bash
YAMAA_CART_T_ODM=/path/to/car-t-openclinica.xml \
YAMAA_KN189_ODM_ARCHIVE=/path/to/kn189_odm.tar.gz \
PYTHONPATH=python/src uv run \
  --with-requirements python/requirements-odm-test.txt \
  python -m pytest python/tests/odm/test_real_data.py
```
