"""The R020 parquet profile: one typed container two runtimes read alike.

R020-28 does not fix these bytes, so nothing here tries to: the schema,
the column order, the row order, the nulls, and the values are what two
runtimes must agree on, and each is written explicitly rather than left to
a writer's default.
"""

from __future__ import annotations

import io

import pyarrow as pa
import pyarrow.parquet as pq

from yamaa.artifacts.output import Artifact
from yamaa.specification.models import ColumnType

# R020-20 maps each declared type to exactly one physical and logical type.
# A `datetime` is a reading on a wall clock, so its Timestamp carries no
# zone and is not adjusted to UTC (R020-24).
_ARROW: dict[ColumnType, pa.DataType] = {
    "str": pa.string(),
    "int": pa.int64(),
    "float": pa.float64(),
    "date": pa.date32(),
    "datetime": pa.timestamp("us"),
}


def parquet_schema(artifact: Artifact) -> pa.Schema:
    """Return the artifact's fields, in `output.columns` order, all optional."""
    return pa.schema(
        [
            # R020-21: every field is optional, because every column type
            # admits a missing value.
            pa.field(column.name, _ARROW[column.type], nullable=True)
            for column in artifact.columns
        ]
    )


def render_parquet(artifact: Artifact) -> bytes:
    """Render one artifact to uncompressed Parquet under the R020 mapping."""
    table = artifact.frame.to_arrow().cast(parquet_schema(artifact))
    buffer = io.BytesIO()
    pq.write_table(
        table,
        buffer,
        # R020-27: uncompressed pages, and no key-value metadata of the
        # implementation's own. `store_schema` would add the writer's Arrow
        # schema beside the Parquet one this rule already fixes.
        compression="none",
        store_schema=False,
    )
    return buffer.getvalue()


def read_parquet(content: bytes) -> pa.Table:
    """Read artifact bytes back, for the comparison R020-26 requires."""
    return pq.read_table(io.BytesIO(content))
