"""Write the Parquet inputs that the installed host suites read.

Staging writes these files beside the suites' other inputs, so that no
Parquet file is committed outside benchmarks/ (#3496). Each is assembled
here from the Parquet format with the standard library alone, so staging
needs no Parquet library and writes the same bytes on every platform: one
row group of uncompressed V1 pages of PLAIN values with RLE levels, under a
Thrift Compact Protocol footer that names each type as pyarrow names it.
Five types exist only in an embedded Arrow schema, so those files carry one,
written here as an Arrow IPC message. No expected output is read or written;
the suites specify their own values and findings. The Rust tests write the
same inputs, among others, at test time
(rust/crates/yamaa-application/tests/support/parquet_files.rs).

Run explicitly to inspect the inputs:
python -m tools.staging.build_parquet_source_inputs <new directory>
"""

import argparse
import base64
import csv
import struct
from collections import namedtuple
from pathlib import Path

from tools.paths import ROOT

# Physical types, repetitions, converted types, LogicalType and TimeUnit
# members and encodings, as the format numbers them.
BOOLEAN, INT32, INT64, FLOAT, DOUBLE = 0, 1, 2, 4, 5
BYTE_ARRAY, FIXED_LEN_BYTE_ARRAY = 6, 7
REQUIRED, OPTIONAL, REPEATED = 0, 1, 2
UTF8, MAP, LIST, DECIMAL = 0, 1, 3, 5
TIMESTAMP_MILLIS, TIMESTAMP_MICROS, UINT_64 = 9, 10, 14
STRING, MAP_TYPE, LIST_TYPE, DECIMAL_TYPE, TIMESTAMP, INTEGER = 1, 2, 3, 5, 8, 10
VARIANT, GEOMETRY, GEOGRAPHY = 16, 17, 18
MILLIS, MICROS = 1, 2
PLAIN, RLE = 0, 3
# The well-known binary of the point (1 2).
POINT = bytes.fromhex("0101000000000000000000f03f0000000000000040")


def _varint(value):
    encoded = bytearray()
    while value >= 0x80:
        encoded.append(value & 0x7F | 0x80)
        value >>= 7
    encoded.append(value)
    return bytes(encoded)


def _zigzag(value):
    return _varint(value << 1 ^ value >> 63)


class _Fields:
    """A Thrift Compact Protocol struct, written one field at a time."""

    def __init__(self):
        self._bytes = bytearray()
        self._last = 0

    def _header(self, field, kind):
        if 0 < field - self._last <= 15:
            self._bytes.append((field - self._last) << 4 | kind)
        else:
            self._bytes.append(kind)
            self._bytes += _zigzag(field)
        self._last = field

    def boolean(self, field, value):
        self._header(field, 1 if value else 2)
        return self

    def byte(self, field, value):
        self._header(field, 3)
        self._bytes.append(value)
        return self

    def i32(self, field, value):
        self._header(field, 5)
        self._bytes += _zigzag(value)
        return self

    def i64(self, field, value):
        self._header(field, 6)
        self._bytes += _zigzag(value)
        return self

    def binary(self, field, value):
        self._header(field, 8)
        self._bytes += _varint(len(value)) + value
        return self

    def list(self, field, kind, items):
        """A list of encoded elements of wire type `kind`."""
        self._header(field, 9)
        if len(items) < 15:
            self._bytes.append(len(items) << 4 | kind)
        else:
            self._bytes += bytes([0xF0 | kind]) + _varint(len(items))
        self._bytes += b"".join(items)
        return self

    def struct(self, field, value):
        self._header(field, 12)
        self._bytes += value.end()
        return self

    def end(self):
        return bytes(self._bytes) + b"\0"


def _logical(member, value=None):
    """A union holding `member`, of struct `value`."""
    return _Fields().struct(member, value or _Fields())


def _timestamp(adjusted, unit):
    """A TIMESTAMP logical type, adjusted to UTC or not, of TimeUnit `unit`."""
    value = _Fields().boolean(1, adjusted).struct(2, _logical(unit))
    return _logical(TIMESTAMP, value)


def _element(
    name,
    repetition,
    *,
    physical=None,
    length=None,
    children=None,
    converted=None,
    scale=None,
    precision=None,
    logical=None,
):
    """A SchemaElement."""
    element = _Fields()
    if physical is not None:
        element.i32(1, physical)
    if length is not None:
        element.i32(2, length)
    element.i32(3, repetition).binary(4, name.encode())
    for field, value in ((5, children), (6, converted), (7, scale), (8, precision)):
        if value is not None:
            element.i32(field, value)
    if logical is not None:
        element.struct(10, logical)
    return element.end()


# One leaf column: its path, physical type, present values, a definition
# level for each value slot and the maximum one, and its repetition levels,
# empty unless the leaf repeats.
Leaf = namedtuple("Leaf", "path physical values definitions defined repetitions")


def _flat(name, physical, values, repetition=OPTIONAL, **annotation):
    """A leaf column of `values`, each None where missing."""
    element = _element(name, repetition, physical=physical, **annotation)
    present = [value for value in values if value is not None]
    if repetition == REQUIRED:
        return [element], [Leaf((name,), physical, present, [0] * len(values), 0, [])]
    definitions = [0 if value is None else 1 for value in values]
    return [element], [Leaf((name,), physical, present, definitions, 1, [])]


def _text(name, values):
    """A UTF8 STRING column of `values`, each None where missing."""
    values = [None if value is None else value.encode() for value in values]
    logical = _logical(STRING)
    return _flat(name, BYTE_ARRAY, values, converted=UTF8, logical=logical)


def _integer(name, value=1):
    return _flat(name, INT64, [value])


def _group(name, children, logical=None):
    """An OPTIONAL group of one present row holding `children`."""
    elements = [_element(name, OPTIONAL, children=len(children), logical=logical)]
    leaves = []
    for child_elements, child_leaves in children:
        elements += child_elements
        for leaf in child_leaves:
            definitions = [level + 1 for level in leaf.definitions]
            leaves.append(
                leaf._replace(
                    path=(name, *leaf.path),
                    definitions=definitions,
                    defined=leaf.defined + 1,
                )
            )
    return elements, leaves


def _list(name, items):
    """A LIST of one row holding the 64-bit integers `items`, as pyarrow nests it."""
    elements = [
        _element(
            name, OPTIONAL, children=1, converted=LIST, logical=_logical(LIST_TYPE)
        ),
        _element("list", REPEATED, children=1),
        _element("element", OPTIONAL, physical=INT64),
    ]
    repetitions = [0] + [1] * (len(items) - 1)
    leaf = Leaf(
        (name, "list", "element"), INT64, items, [3] * len(items), 3, repetitions
    )
    return elements, [leaf]


def _map(name):
    """A MAP of one row holding the one entry `key: 2`, as pyarrow nests it."""
    elements = [
        _element(name, OPTIONAL, children=1, converted=MAP, logical=_logical(MAP_TYPE)),
        _element("key_value", REPEATED, children=2),
        _element(
            "key",
            REQUIRED,
            physical=BYTE_ARRAY,
            converted=UTF8,
            logical=_logical(STRING),
        ),
        _element("value", OPTIONAL, physical=INT64),
    ]
    leaves = [
        Leaf((name, "key_value", "key"), BYTE_ARRAY, [b"key"], [2], 2, [0]),
        Leaf((name, "key_value", "value"), INT64, [2], [3], 3, [0]),
    ]
    return elements, leaves


def _decimal(width, precision):
    """The decimal 1.23 of scale 2 in `width` bytes, as pyarrow stores it."""
    value = (123).to_bytes(width, "big", signed=True)
    return _flat(
        "FIELD",
        FIXED_LEN_BYTE_ARRAY,
        [value],
        length=width,
        converted=DECIMAL,
        scale=2,
        precision=precision,
        logical=_logical(DECIMAL_TYPE, _Fields().i32(1, 2).i32(2, precision)),
    )


def _levels(levels, maximum):
    """RLE-hybrid levels, one bit-packed run, after their 4-byte length."""
    if not maximum:
        return b""
    width = maximum.bit_length()
    groups = -(-len(levels) // 8)
    packed = 0
    for index, level in enumerate(levels):
        packed |= level << index * width
    run = _varint(groups << 1 | 1) + packed.to_bytes(groups * width, "little")
    return struct.pack("<I", len(run)) + run


def _plain(physical, values):
    if physical == BOOLEAN:
        packed = sum(int(value) << index for index, value in enumerate(values))
        return packed.to_bytes(-(-len(values) // 8), "little")
    if physical == BYTE_ARRAY:
        return b"".join(struct.pack("<I", len(value)) + value for value in values)
    if physical == FIXED_LEN_BYTE_ARRAY:
        return b"".join(values)
    code = {INT32: "<i", INT64: "<q", FLOAT: "<f", DOUBLE: "<d"}[physical]
    return b"".join(struct.pack(code, value) for value in values)


def _file(columns, rows, schema=None):
    """One row group of `rows` rows of `columns`, each leaf in one page,
    with the embedded Arrow schema `schema` if one is given."""
    elements, leaves = [], []
    for column_elements, column_leaves in columns:
        elements += column_elements
        leaves += column_leaves
    data = bytearray(b"PAR1")
    chunks = []
    for leaf in leaves:
        body = (
            _levels(leaf.repetitions, 1 if leaf.repetitions else 0)
            + _levels(leaf.definitions, leaf.defined)
            + _plain(leaf.physical, leaf.values)
        )
        page = _Fields().i32(1, len(leaf.definitions)).i32(2, PLAIN)
        page = page.i32(3, RLE).i32(4, RLE)
        header = _Fields().i32(1, 0).i32(2, len(body)).i32(3, len(body))
        header = header.struct(5, page).end()
        offset = len(data)
        data += header + body
        size = len(header) + len(body)
        path = [_varint(len(name.encode())) + name.encode() for name in leaf.path]
        metadata = _Fields().i32(1, leaf.physical)
        metadata.list(2, 5, [_zigzag(PLAIN), _zigzag(RLE)]).list(3, 8, path)
        metadata.i32(4, 0).i64(5, len(leaf.definitions))
        metadata.i64(6, size).i64(7, size).i64(9, offset)
        chunks.append(_Fields().i64(2, offset).struct(3, metadata).end())
    group = _Fields().list(1, 12, chunks).i64(2, len(data) - 4).i64(3, rows).end()
    root = _element("schema", REQUIRED, children=len(columns))
    footer = _Fields().i32(1, 1).list(2, 12, [root, *elements]).i64(3, rows)
    footer.list(4, 12, [group])
    if schema is not None:
        pair = _Fields().binary(1, b"ARROW:schema").binary(2, schema).end()
        footer.list(5, 12, [pair])
    footer = footer.end()
    return bytes(data + footer + struct.pack("<I", len(footer)) + b"PAR1")


class _Flatbuffer:
    """A flatbuffer written front to back: each object follows the offsets
    that refer to it, and each value sits at a multiple of its size."""

    def __init__(self):
        self.data = bytearray(4)

    def _pad(self, alignment):
        self.data += bytes(-len(self.data) % alignment)

    def table(self, fields):
        """A table of `fields`, each `(slot, code, value)`: a scalar of
        struct code `code`, or, for code "o", an offset to the object that
        the callable `value` writes after the table."""
        layout, size = [], 4
        for slot, code, value in fields:
            width = 4 if code == "o" else struct.calcsize(code)
            size += -size % width
            layout.append((slot, code, value, size))
            size += width
        slots = max((slot for slot, _, _ in fields), default=-1) + 1
        places = {slot: at for slot, _, _, at in layout}
        vtable = [4 + 2 * slots, size, *(places.get(slot, 0) for slot in range(slots))]
        self._pad(2)
        vtable_at = len(self.data)
        self.data += struct.pack(f"<{len(vtable)}H", *vtable)
        self._pad(4)
        table_at = len(self.data)
        self.data += bytes(size)
        struct.pack_into("<i", self.data, table_at, table_at - vtable_at)
        for _, code, value, at in layout:
            if code != "o":
                struct.pack_into("<" + code, self.data, table_at + at, value)
        for _, code, value, at in layout:
            if code == "o":
                child = value()
                struct.pack_into("<I", self.data, table_at + at, child - table_at - at)
        return table_at

    def string(self, text):
        self._pad(4)
        at = len(self.data)
        encoded = text.encode()
        self.data += struct.pack("<I", len(encoded)) + encoded + b"\0"
        return at

    def vector(self, writers):
        """A vector of offsets to the objects that `writers` write after it."""
        self._pad(4)
        at = len(self.data)
        self.data += struct.pack("<I", len(writers)) + bytes(4 * len(writers))
        for index, writer in enumerate(writers):
            child = writer()
            struct.pack_into(
                "<I", self.data, at + 4 + 4 * index, child - at - 4 - 4 * index
            )
        return at

    def finish(self, root):
        struct.pack_into("<I", self.data, 0, root())
        self._pad(8)
        return bytes(self.data)


# Each embedded type: its member of the IPC Type union (Decimal 7,
# FixedSizeList 16, ListView 25, LargeListView 26) and its table's scalars.
LATER_TYPES = {
    "decimal32": (7, [(0, "i", 9), (1, "i", 2), (2, "i", 32)]),
    "decimal64": (7, [(0, "i", 18), (1, "i", 2), (2, "i", 64)]),
    "fixed-list": (16, [(0, "i", 2)]),
    "list-view": (25, []),
    "large-list-view": (26, []),
}


def _arrow_schema(name):
    """The base64 ARROW:schema value of one nullable field, FIELD, of the
    type `name`, as writers embed one: an encapsulated IPC Schema message,
    its continuation marker, length and flatbuffer padded to eight bytes. A
    list's values are a nullable int64 field named `item`."""
    buffer = _Flatbuffer()
    member, scalars = LATER_TYPES[name]

    def field(field_name, member, scalars, children):
        return lambda: buffer.table(
            [
                (0, "o", lambda: buffer.string(field_name)),
                (1, "B", 1),
                (2, "B", member),
                (3, "o", lambda: buffer.table(scalars)),
                (5, "o", lambda: buffer.vector(children)),
            ]
        )

    item = field("item", 2, [(0, "i", 64), (1, "B", 1)], [])
    children = [] if name.startswith("decimal") else [item]
    schema = [
        (1, "o", lambda: buffer.vector([field("FIELD", member, scalars, children)]))
    ]
    # MetadataVersion V5, and a Schema header.
    message = [(0, "h", 4), (1, "B", 1), (2, "o", lambda: buffer.table(schema))]
    flatbuffer = buffer.finish(lambda: buffer.table(message))
    return base64.b64encode(
        b"\xff\xff\xff\xff" + struct.pack("<I", len(flatbuffer)) + flatbuffer
    )


def _ordered_sum():
    """The original ordered-sum source in a Parquet container: text fields,
    and the LBSTRESN result as a double."""
    original = ROOT / "benchmarks/adam-adlb-ordered-sum/input/lb.csv"
    with original.open(newline="", encoding="ascii") as stream:
        rows = list(csv.DictReader(stream))
    columns = []
    for name in rows[0]:
        values = [row[name] or None for row in rows]
        if name == "LBSTRESN":
            numbers = [None if value is None else float(value) for value in values]
            columns.append(_flat(name, DOUBLE, numbers))
        else:
            columns.append(_text(name, values))
    return _file(columns, len(rows))


def inputs():
    """Each input's name and bytes."""
    # A string payload that is not UTF-8, after a field the closed mapping
    # refuses or that holds a fractional second.
    invalid = _flat(
        "TEXT", BYTE_ARRAY, [b"\xff\xff"], converted=UTF8, logical=_logical(STRING)
    )
    microseconds = {"converted": TIMESTAMP_MICROS, "logical": _timestamp(False, MICROS)}
    files = {
        "text": _file([_flat("I", INT64, [1, 2]), _text("S", ["", "X"])], 2),
        "ordered-sum": _ordered_sum(),
        "empty-name": _file([_integer("")], 1),
        "duplicate": _file([_integer("I", 1), _integer("I", 2)], 1),
        "mixed-struct": _file(
            [
                _integer("I"),
                _group("S", [_integer("left", 2), _text("right", ["text"])]),
                _integer("AFTER"),
            ],
            1,
        ),
        "mixed-map": _file([_integer("I"), _map("M"), _integer("AFTER")], 1),
        "mixed-empty": _file([_integer(""), _map("M")], 1),
        "mixed-duplicate": _file([_integer("I"), _integer("I"), _map("M")], 1),
        "utf8": _file([invalid], 1),
        "utf8-bool": _file([_flat("OTHER", BOOLEAN, [True]), invalid], 1),
        "utf8-time": _file([_flat("OTHER", INT64, [1], **microseconds), invalid], 1),
    }
    unsupported = {
        "int32": _flat("FIELD", INT32, [1]),
        "uint64": _flat(
            "FIELD",
            INT64,
            [1],
            converted=UINT_64,
            logical=_logical(INTEGER, _Fields().byte(1, 64).boolean(2, False)),
        ),
        "float32": _flat("FIELD", FLOAT, [1.25]),
        "binary": _flat("FIELD", BYTE_ARRAY, [b"x"]),
        "milliseconds": _flat(
            "FIELD",
            INT64,
            [1000],
            converted=TIMESTAMP_MILLIS,
            logical=_timestamp(False, MILLIS),
        ),
        "timezone": _flat(
            "FIELD",
            INT64,
            [1_000_000],
            converted=TIMESTAMP_MICROS,
            logical=_timestamp(True, MICROS),
        ),
        "list": _list("FIELD", [1]),
        "fixed-list": _list("FIELD", [1, 2]),
        "struct": _group("FIELD", [_integer("item")]),
        "decimal": _decimal(5, 10),
        "decimal32": _decimal(4, 9),
        "decimal64": _decimal(8, 18),
        "list-view": _list("FIELD", [1]),
        "large-list-view": _list("FIELD", [1]),
        # Logical types that the Parquet release the engine pins does not
        # know (#3155).
        "geometry": _flat("FIELD", BYTE_ARRAY, [POINT], logical=_logical(GEOMETRY)),
        "geography": _flat("FIELD", BYTE_ARRAY, [POINT], logical=_logical(GEOGRAPHY)),
        "variant": _group(
            "FIELD",
            [
                _flat("metadata", BYTE_ARRAY, [b"\x01\x00\x00"], REQUIRED),
                _flat("value", BYTE_ARRAY, [b"\x0c\x01"], REQUIRED),
            ],
            logical=_logical(VARIANT, _Fields().byte(1, 1)),
        ),
    }
    for name, column in unsupported.items():
        schema = _arrow_schema(name) if name in LATER_TYPES else None
        files["unsupported-" + name] = _file([column], 1, schema)
    return files


def build(destination: Path):
    destination.mkdir(parents=True, exist_ok=False)
    for name, content in inputs().items():
        (destination / (name + ".parquet")).write_bytes(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    build(parser.parse_args().destination)
