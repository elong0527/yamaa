//! The native footer model: an admitted footer's file metadata, read from
//! its compact Thrift fields (`compact`, typed by `wire`'s field table), with
//! the checks that parquet 55.2.0's metadata decoder makes. A footer this
//! decoder refuses is not one readable Parquet file (PARQUET-5).
//!
//! The checks are those of the generated Thrift readers and of the
//! conversions after them:
//!
//! - every required field of each struct it reads is present, and a known
//!   text field holds UTF-8;
//! - a union holds exactly one field, one it knows: the logical type, a
//!   time unit, a column order, the encryption algorithm and a column's
//!   crypto metadata. A variant, geometry or geography annotation that the
//!   reader refuses as a type of its own (`wire::accepted`) reads as no
//!   logical type; any column order reads as the type-defined one;
//! - each physical type, repetition, converted type, encoding, codec and
//!   page type it converts is one the format codes, and the closed profile
//!   lists the physical types, encodings, codecs and page types
//!   (`closed_profile`);
//! - the schema list is one tree, whose every element but the root has a
//!   repetition, and each primitive's annotations are ones its physical type
//!   can carry;
//! - each row group has a chunk for every leaf, with column metadata, and a
//!   chunk's statistics hold no negative null count and no minimum or
//!   maximum outside its type's width (`closed_profile::statistics`).
//!
//! What the decoder does not convert, it does not check.
use super::closed_profile;
use super::compact::{Error, Fields, Sequence, Value};
use super::format::{Codec, Converted, Encoding, Logical, Physical, TimeUnit};

/// A leaf or group's repetition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Repetition {
    Required,
    Optional,
    Repeated,
}

/// An admitted footer's file metadata.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Footer {
    pub num_rows: i64,
    /// The key-value metadata, in order.
    pub key_values: Vec<(String, Option<String>)>,
    pub schema: Schema,
    pub row_groups: Vec<RowGroup>,
}

/// The schema tree: its nodes, the message root first and each node's
/// children after it, depth first, and its leaves in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Schema {
    pub nodes: Vec<Node>,
    pub leaves: Vec<Leaf>,
}

impl Schema {
    /// Whether the path of `leaf` is `name`: the names from the message
    /// root's child above the leaf down to it, joined by `.`. The path is
    /// read from the leaf up and never built: a path held by each leaf
    /// would copy the names above it once for every leaf below them, which
    /// a footer of nested groups and many leaves makes far larger than the
    /// footer itself (#4782).
    pub fn named(&self, leaf: &Leaf, name: &str) -> bool {
        let mut above = name.as_bytes();
        for (index, part) in self.names(leaf).enumerate() {
            // A `.` joins each name to the one below it.
            let joined = match index {
                0 => Some(above),
                _ => above.strip_suffix(b"."),
            };
            let Some(rest) = joined.and_then(|rest| rest.strip_suffix(part.as_bytes())) else {
                return false;
            };
            above = rest;
        }
        above.is_empty()
    }

    /// The names of the path of `leaf` ([`Self::named`]), from the leaf up.
    fn names<'a>(&'a self, leaf: &Leaf) -> impl Iterator<Item = &'a str> {
        let mut next = Some(leaf.node);
        std::iter::from_fn(move || {
            let node = &self.nodes[next?];
            next = (node.parent != 0).then_some(node.parent);
            Some(node.name.as_str())
        })
    }

    /// The bytes of the path of `leaf` ([`Self::named`]), or `None` past the
    /// platform's count.
    pub fn path_bytes(&self, leaf: &Leaf) -> Option<usize> {
        // One byte joins each name to the one above it.
        self.names(leaf)
            .try_fold(0usize, |bytes, name| bytes.checked_add(name.len() + 1))
            .map(|bytes| bytes - 1)
    }

    /// The path of `leaf` ([`Self::named`]), built for a caller that has
    /// admitted its bytes ([`Self::path_bytes`]).
    pub fn path(&self, leaf: &Leaf) -> String {
        let mut names: Vec<_> = self.names(leaf).collect();
        names.reverse();
        names.join(".")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Node {
    pub name: String,
    /// The position in `Schema::nodes` of the group that holds the node; the
    /// message root, which no group holds, names itself.
    pub parent: usize,
    /// `None` for the message root alone.
    pub repetition: Option<Repetition>,
    pub converted: Converted,
    pub logical: Option<Logical>,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Kind {
    /// A group, and its children's positions in `Schema::nodes`.
    Group(Vec<usize>),
    /// A primitive, and its position in `Schema::leaves`.
    Primitive(usize),
}

/// One leaf column: its primitive's physical type and annotations, where it
/// sits, and its maximum levels. Its path is read from its node up
/// (`Schema::named`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Leaf {
    pub node: usize,
    /// The position of the message root's child above it.
    pub root: usize,
    pub physical: Physical,
    pub type_length: i32,
    pub precision: i32,
    pub scale: i32,
    pub converted: Converted,
    pub logical: Option<Logical>,
    pub max_definition: i16,
    pub max_repetition: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowGroup {
    pub num_rows: i64,
    pub columns: Vec<Chunk>,
}

/// A column chunk's metadata, as the reader uses it. The chunk's own physical
/// type and path are checked, and otherwise unused: its leaf's count.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Chunk {
    pub encodings: Vec<Encoding>,
    pub codec: Codec,
    pub num_values: i64,
    pub data_page_offset: i64,
    pub dictionary_page_offset: Option<i64>,
    pub total_compressed_size: i64,
}

/// Decode the admitted footer `held`, the compact Thrift `FileMetaData` that
/// framing admitted (`framing::Framing::metadata`).
pub(super) fn decode(held: &[u8]) -> Result<Footer, Error> {
    let (root, _) = super::compact::parse(held)?;
    let file = fields(root)?;
    required(integer(&file, 1)?)?;
    let elements = required(list(&file, 2)?)?;
    let num_rows = required(integer(&file, 3)?)?;
    let groups = required(list(&file, 4)?)?;
    let key_values = match list(&file, 5)? {
        Some(items) => key_values(items)?,
        None => Vec::new(),
    };
    text(&file, 6)?;
    let orders = list(&file, 7)?;
    for order in orders.into_iter().flatten() {
        // Shown to the decoder as the type-defined order, whatever variant
        // it holds (`wire::Presentation`).
        variant(&fields(order?)?, |_| true)?;
    }
    if let Some(algorithm) = structure(&file, 8)? {
        let aes = fields(variant(&fields(algorithm)?, |id| matches!(id, 1 | 2))?.1)?;
        bytes(&aes, 1)?;
        bytes(&aes, 2)?;
        boolean(&aes, 3)?;
    }
    let schema = schema(elements)?;
    if orders.is_some_and(|orders| orders.len() != schema.leaves.len()) {
        return Err(Error::Malformed);
    }
    let row_groups = groups
        .into_iter()
        .map(|group| row_group(group?, &schema))
        .collect::<Result<_, _>>()?;
    Ok(Footer {
        num_rows,
        key_values,
        schema,
        row_groups,
    })
}

fn key_values(items: Sequence<'_>) -> Result<Vec<(String, Option<String>)>, Error> {
    items
        .into_iter()
        .map(|item| {
            let item = fields(item?)?;
            Ok((required(text(&item, 1)?)?, text(&item, 2)?))
        })
        .collect()
}

fn row_group(value: Value<'_>, schema: &Schema) -> Result<RowGroup, Error> {
    let group = fields(value)?;
    let columns = required(list(&group, 1)?)?;
    required(integer(&group, 2)?)?;
    let num_rows = required(integer(&group, 3)?)?;
    for sorting in list(&group, 4)?.into_iter().flatten() {
        let sorting = fields(sorting?)?;
        required(integer(&sorting, 1)?)?;
        required(boolean(&sorting, 2)?)?;
        required(boolean(&sorting, 3)?)?;
    }
    if columns.len() != schema.leaves.len() {
        return Err(Error::Malformed);
    }
    let columns = columns
        .into_iter()
        .map(|chunk| self::chunk(chunk?))
        .collect::<Result<_, _>>()?;
    Ok(RowGroup { num_rows, columns })
}

fn chunk(value: Value<'_>) -> Result<Chunk, Error> {
    let chunk = fields(value)?;
    text(&chunk, 1)?;
    required(integer(&chunk, 2)?)?;
    if let Some(crypto) = structure(&chunk, 8)? {
        let (id, key) = variant(&fields(crypto)?, |id| matches!(id, 1 | 2))?;
        if id == 2 {
            let key = fields(key)?;
            texts(required(list(&key, 1)?)?)?;
            bytes(&key, 2)?;
        }
    }
    let meta = fields(required(structure(&chunk, 3)?)?)?;
    let physical = closed_profile::physical(required(integer(&meta, 1)?)?);
    let physical = physical.ok_or(Error::Malformed)?;
    let encodings = required(list(&meta, 2)?)?
        .into_iter()
        .map(|code| closed_profile::encoding(integer_value(code?)?).ok_or(Error::Malformed))
        .collect::<Result<_, _>>()?;
    texts(required(list(&meta, 3)?)?)?;
    let codec = closed_profile::codec(required(integer(&meta, 4)?)?);
    let (codec, _) = codec.ok_or(Error::Malformed)?;
    let num_values = required(integer(&meta, 5)?)?;
    required(integer(&meta, 6)?)?;
    let total_compressed_size = required(integer(&meta, 7)?)?;
    if let Some(items) = list(&meta, 8)? {
        key_values(items)?;
    }
    let data_page_offset = required(integer(&meta, 9)?)?;
    let dictionary_page_offset = integer(&meta, 11)?;
    if let Some(statistics) = structure(&meta, 12)? {
        self::statistics(statistics, physical)?;
    }
    for stats in list(&meta, 13)?.into_iter().flatten() {
        let stats = fields(stats?)?;
        closed_profile::page(required(integer(&stats, 1)?)?).ok_or(Error::Malformed)?;
        closed_profile::encoding(required(integer(&stats, 2)?)?).ok_or(Error::Malformed)?;
        required(integer(&stats, 3)?)?;
    }
    Ok(Chunk {
        encodings,
        codec,
        num_values,
        data_page_offset,
        dictionary_page_offset,
        total_compressed_size,
    })
}

/// A column chunk's or data page's statistics, of values of `physical`: no
/// negative null count, and a minimum and maximum no shorter than the type's
/// width, nor longer when the profile says exactly. The deprecated minimum
/// and maximum count only when neither current one is present.
pub(super) fn statistics(value: Value<'_>, physical: Physical) -> Result<(), Error> {
    let statistics = fields(value)?;
    if integer(&statistics, 3)?.is_some_and(|nulls| nulls < 0) {
        return Err(Error::Malformed);
    }
    integer(&statistics, 4)?;
    boolean(&statistics, 7)?;
    boolean(&statistics, 8)?;
    let (max, min) = match (bytes(&statistics, 5)?, bytes(&statistics, 6)?) {
        (None, None) => (bytes(&statistics, 1)?, bytes(&statistics, 2)?),
        current => current,
    };
    if let Some((width, exact)) = closed_profile::statistics(physical) {
        let fits = |bound: &[u8]| bound.len() == width || (!exact && bound.len() > width);
        if !max.into_iter().chain(min).all(fits) {
            return Err(Error::Malformed);
        }
    }
    Ok(())
}

/// One schema element, as the Thrift reader holds it before the tree is
/// built.
struct Element {
    physical: Option<i64>,
    type_length: Option<i64>,
    repetition: Option<i64>,
    name: String,
    children: Option<i64>,
    converted: Option<i64>,
    scale: Option<i64>,
    precision: Option<i64>,
    logical: Option<Logical>,
}

fn element(value: Value<'_>) -> Result<Element, Error> {
    let element = fields(value)?;
    integer(&element, 9)?;
    Ok(Element {
        physical: integer(&element, 1)?,
        type_length: integer(&element, 2)?,
        repetition: integer(&element, 3)?,
        name: required(text(&element, 4)?)?,
        children: integer(&element, 5)?,
        converted: integer(&element, 6)?,
        scale: integer(&element, 7)?,
        precision: integer(&element, 8)?,
        logical: logical(value, &element)?,
    })
}

/// A schema element's logical type. A union of one variant, geometry or
/// geography annotation that parquet 57 admitted reads as none: the reader
/// refuses that leaf's type itself (`wire::accepted`, PARQUET-7).
fn logical(value: Value<'_>, element: &Struct<'_>) -> Result<Option<Logical>, Error> {
    let Some(union) = structure(element, 10)? else {
        return Ok(None);
    };
    let union = fields(union)?;
    if let ((1, Some(later @ 16..=18)), Value::Struct(held)) = ((union.count, union.first), value) {
        if super::wire::accepted(held, later)? {
            return Ok(None);
        }
    }
    let (id, inner) = variant(&union, |id| matches!(id, 1..=8 | 10..=15))?;
    let inner = fields(inner)?;
    let time = |inner: &Struct<'_>| -> Result<(bool, TimeUnit), Error> {
        let utc = required(boolean(inner, 1)?)?;
        let unit = fields(required(structure(inner, 2)?)?)?;
        let unit = match variant(&unit, |id| matches!(id, 1..=3))?.0 {
            1 => TimeUnit::Millis,
            2 => TimeUnit::Micros,
            _ => TimeUnit::Nanos,
        };
        Ok((utc, unit))
    };
    Ok(Some(match id {
        1 => Logical::String,
        2 => Logical::Map,
        3 => Logical::List,
        4 => Logical::Enum,
        5 => Logical::Decimal {
            scale: int32(required(integer(&inner, 1)?)?)?,
            precision: int32(required(integer(&inner, 2)?)?)?,
        },
        6 => Logical::Date,
        7 => {
            let (utc, unit) = time(&inner)?;
            Logical::Time { utc, unit }
        }
        8 => {
            let (utc, unit) = time(&inner)?;
            Logical::Timestamp { utc, unit }
        }
        10 => Logical::Integer {
            bits: i8::try_from(required(integer(&inner, 1)?)?).map_err(|_| Error::Malformed)?,
            signed: required(boolean(&inner, 2)?)?,
        },
        11 => Logical::Unknown,
        12 => Logical::Json,
        13 => Logical::Bson,
        14 => Logical::Uuid,
        _ => Logical::Float16,
    }))
}

/// Build the schema tree from its flat element list.
fn schema(list: Sequence<'_>) -> Result<Schema, Error> {
    let elements = list
        .into_iter()
        .map(|value| element(value?))
        .collect::<Result<Vec<_>, _>>()?;
    let mut schema = Schema {
        nodes: Vec::new(),
        leaves: Vec::new(),
    };
    let (next, _) = node(&elements, 0, 0, &mut schema)?;
    // Exactly one root, holding every element.
    if next != elements.len() {
        return Err(Error::Malformed);
    }
    let Kind::Group(roots) = schema.nodes[0].kind.clone() else {
        return Err(Error::Malformed);
    };
    for (root, &child) in roots.iter().enumerate() {
        levels(&mut schema, child, root, (0, 0))?;
    }
    Ok(schema)
}

/// Add the node of element `index`, a child of the node at `parent`, and
/// every node below it, to `schema`; return the position of the element
/// after its subtree, and the node's.
fn node(
    elements: &[Element],
    index: usize,
    parent: usize,
    schema: &mut Schema,
) -> Result<(usize, usize), Error> {
    let element = elements.get(index).ok_or(Error::Malformed)?;
    let root = index == 0;
    let position = schema.nodes.len();
    if root && element.children.is_none_or(|children| children == 0) {
        schema.nodes.push(Node {
            name: element.name.clone(),
            parent,
            repetition: None,
            converted: Converted::None,
            logical: None,
            kind: Kind::Group(Vec::new()),
        });
        return Ok((index + 1, position));
    }
    let converted = match element.converted {
        None => Converted::None,
        Some(code) => converted(code).ok_or(Error::Malformed)?,
    };
    let logical = element.logical;
    if let Some(Logical::Integer { bits, .. }) = logical {
        if !matches!(bits, 8 | 16 | 32 | 64) {
            return Err(Error::Malformed);
        }
    }
    match element.children {
        None | Some(0) => {
            let repetition = repetition(element.repetition.ok_or(Error::Malformed)?)?;
            let Some(physical) = element.physical else {
                schema.nodes.push(Node {
                    name: element.name.clone(),
                    parent,
                    repetition: Some(repetition),
                    converted: populated(converted, logical),
                    logical,
                    kind: Kind::Group(Vec::new()),
                });
                return Ok((index + 1, position));
            };
            let physical = closed_profile::physical(physical).ok_or(Error::Malformed)?;
            let primitive = Primitive {
                physical,
                length: int32(element.type_length.unwrap_or(-1))?,
                scale: int32(element.scale.unwrap_or(-1))?,
                precision: int32(element.precision.unwrap_or(-1))?,
                converted,
                logical,
            };
            primitive.check()?;
            schema.nodes.push(Node {
                name: element.name.clone(),
                parent,
                repetition: Some(repetition),
                converted: populated(converted, logical),
                logical,
                kind: Kind::Primitive(schema.leaves.len()),
            });
            schema.leaves.push(Leaf {
                node: position,
                root: 0,
                physical,
                type_length: primitive.length,
                precision: primitive.precision,
                scale: primitive.scale,
                converted: populated(converted, logical),
                logical,
                max_definition: 0,
                max_repetition: 0,
            });
            Ok((index + 1, position))
        }
        Some(count) => {
            let repetition = element.repetition.map(self::repetition).transpose()?;
            schema.nodes.push(Node {
                name: element.name.clone(),
                parent,
                // The message root holds no repetition, whatever it says.
                repetition: repetition.filter(|_| !root),
                converted: populated(converted, logical),
                logical,
                kind: Kind::Group(Vec::new()),
            });
            let mut next = index + 1;
            let mut children = Vec::new();
            for _ in 0..count {
                let (after, child) = node(elements, next, position, schema)?;
                next = after;
                children.push(child);
            }
            schema.nodes[position].kind = Kind::Group(children);
            Ok((next, position))
        }
    }
}

/// Give each leaf below node `node`, a child of the root at `root`, that
/// child's position and its maximum levels. Every node but the message root
/// has a repetition.
fn levels(
    schema: &mut Schema,
    node: usize,
    root: usize,
    (mut definition, mut repetition): (i16, i16),
) -> Result<(), Error> {
    match schema.nodes[node].repetition.ok_or(Error::Malformed)? {
        Repetition::Required => {}
        Repetition::Optional => definition += 1,
        Repetition::Repeated => {
            definition += 1;
            repetition += 1;
        }
    }
    match schema.nodes[node].kind.clone() {
        Kind::Primitive(leaf) => {
            let leaf = &mut schema.leaves[leaf];
            leaf.root = root;
            leaf.max_definition = definition;
            leaf.max_repetition = repetition;
        }
        Kind::Group(children) => {
            for child in children {
                levels(schema, child, root, (definition, repetition))?;
            }
        }
    }
    Ok(())
}

/// A primitive's physical type and annotations, as the schema builder checks
/// them.
struct Primitive {
    physical: Physical,
    length: i32,
    scale: i32,
    precision: i32,
    converted: Converted,
    logical: Option<Logical>,
}

impl Primitive {
    /// Whether the physical type can carry the annotations: the logical type
    /// agrees with any converted type and suits the physical type, and so
    /// does the converted type, as written.
    fn check(&self) -> Result<(), Error> {
        use Physical as P;
        let fixed = self.physical == P::FixedLenByteArray;
        if fixed && self.length < 0 {
            return Err(Error::Malformed);
        }
        if let Some(logical) = self.logical {
            if self.converted != Converted::None
                && populated(Converted::None, Some(logical)) != self.converted
            {
                return Err(Error::Malformed);
            }
            let suits = match (logical, self.physical) {
                (Logical::Decimal { scale, precision }, _) => {
                    if scale != self.scale || precision != self.precision {
                        return Err(Error::Malformed);
                    }
                    self.decimal()?;
                    true
                }
                (Logical::Time { unit, .. }, P::Int32) => unit == TimeUnit::Millis,
                (Logical::Time { unit, .. }, P::Int64) => unit != TimeUnit::Millis,
                (Logical::Integer { bits, .. }, P::Int32) => bits <= 32,
                (Logical::Integer { bits, .. }, P::Int64) => bits == 64,
                (Logical::Uuid, P::FixedLenByteArray) => self.length == 16,
                (Logical::Float16, P::FixedLenByteArray) => self.length == 2,
                (Logical::Enum | Logical::String | Logical::Json | Logical::Bson, P::ByteArray)
                | (Logical::Date | Logical::Unknown, P::Int32)
                | (Logical::Timestamp { .. }, P::Int64) => true,
                _ => false,
            };
            if !suits {
                return Err(Error::Malformed);
            }
        }
        let suits = match self.converted {
            Converted::None => true,
            Converted::Utf8 | Converted::Bson | Converted::Json | Converted::Enum => {
                self.physical == P::ByteArray
            }
            Converted::Decimal => {
                self.decimal()?;
                true
            }
            Converted::Date
            | Converted::TimeMillis
            | Converted::Uint8
            | Converted::Uint16
            | Converted::Uint32
            | Converted::Int8
            | Converted::Int16
            | Converted::Int32 => self.physical == P::Int32,
            Converted::TimeMicros
            | Converted::TimestampMillis
            | Converted::TimestampMicros
            | Converted::Uint64
            | Converted::Int64 => self.physical == P::Int64,
            Converted::Interval => fixed && self.length == 12,
            Converted::Map | Converted::MapKeyValue | Converted::List => false,
        };
        if suits {
            Ok(())
        } else {
            Err(Error::Malformed)
        }
    }

    /// A decimal's precision and scale suit its physical type.
    fn decimal(&self) -> Result<(), Error> {
        let most = match self.physical {
            Physical::Int32 => 9,
            Physical::Int64 => 18,
            Physical::ByteArray => i32::MAX,
            Physical::FixedLenByteArray => {
                let bits = self.length.checked_mul(8).ok_or(Error::Malformed)?;
                // As the builder computes it, saturating at the widest.
                (2f64.powi(bits - 1) - 1f64).log10().floor() as i32
            }
            _ => return Err(Error::Malformed),
        };
        if self.precision < 1
            || self.scale < 0
            || self.scale > self.precision
            || self.precision > most
        {
            return Err(Error::Malformed);
        }
        Ok(())
    }
}

/// The converted type a node holds: its own, or, when it has none, the one
/// its logical type implies.
fn populated(converted: Converted, logical: Option<Logical>) -> Converted {
    use Converted as C;
    let (Converted::None, Some(logical)) = (converted, logical) else {
        return converted;
    };
    match logical {
        Logical::String => C::Utf8,
        Logical::Map => C::Map,
        Logical::List => C::List,
        Logical::Enum => C::Enum,
        Logical::Decimal { .. } => C::Decimal,
        Logical::Date => C::Date,
        Logical::Time { unit, .. } => match unit {
            TimeUnit::Millis => C::TimeMillis,
            TimeUnit::Micros => C::TimeMicros,
            TimeUnit::Nanos => C::None,
        },
        Logical::Timestamp { unit, .. } => match unit {
            TimeUnit::Millis => C::TimestampMillis,
            TimeUnit::Micros => C::TimestampMicros,
            TimeUnit::Nanos => C::None,
        },
        Logical::Integer { bits, signed } => match (bits, signed) {
            (8, true) => C::Int8,
            (16, true) => C::Int16,
            (32, true) => C::Int32,
            (64, true) => C::Int64,
            (8, false) => C::Uint8,
            (16, false) => C::Uint16,
            (32, false) => C::Uint32,
            (64, false) => C::Uint64,
            // Every other width is refused before a node is built.
            _ => C::None,
        },
        Logical::Json => C::Json,
        Logical::Bson => C::Bson,
        Logical::Uuid | Logical::Float16 | Logical::Unknown => C::None,
    }
}

/// The converted type of format code `code`.
fn converted(code: i64) -> Option<Converted> {
    use Converted as C;
    const CODED: [Converted; 22] = [
        C::Utf8,
        C::Map,
        C::MapKeyValue,
        C::List,
        C::Enum,
        C::Decimal,
        C::Date,
        C::TimeMillis,
        C::TimeMicros,
        C::TimestampMillis,
        C::TimestampMicros,
        C::Uint8,
        C::Uint16,
        C::Uint32,
        C::Uint64,
        C::Int8,
        C::Int16,
        C::Int32,
        C::Int64,
        C::Json,
        C::Bson,
        C::Interval,
    ];
    CODED.get(usize::try_from(code).ok()?).copied()
}

/// The repetition of format code `code`.
fn repetition(code: i64) -> Result<Repetition, Error> {
    match code {
        0 => Ok(Repetition::Required),
        1 => Ok(Repetition::Optional),
        2 => Ok(Repetition::Repeated),
        _ => Err(Error::Malformed),
    }
}

/// A struct's fields, by id: every id the format's footer and page header
/// structs know is below 19.
type Struct<'a> = Fields<'a, 19>;

fn fields(value: Value<'_>) -> Result<Struct<'_>, Error> {
    value.fields()
}

/// The one field of the union `fields`, which must be one it knows; its id
/// and value.
fn variant<'a>(
    fields: &Struct<'a>,
    known: impl Fn(i16) -> bool,
) -> Result<(i16, Value<'a>), Error> {
    match (fields.count, fields.first) {
        (1, Some(id)) if known(id) => Ok((id, field(fields, id).ok_or(Error::Malformed)?)),
        _ => Err(Error::Malformed),
    }
}

fn field<'a>(fields: &Struct<'a>, id: i16) -> Option<Value<'a>> {
    fields
        .values
        .get(usize::try_from(id).ok()?)
        .copied()
        .flatten()
}

fn required<T>(value: Option<T>) -> Result<T, Error> {
    value.ok_or(Error::Malformed)
}

fn int32(value: i64) -> Result<i32, Error> {
    i32::try_from(value).map_err(|_| Error::Malformed)
}

fn integer_value(value: Value<'_>) -> Result<i64, Error> {
    match value {
        Value::Integer(value) => Ok(value),
        _ => Err(Error::Malformed),
    }
}

/// The integer field `id`, if present. `wire` admits a known field only in
/// its declared wire type, so any other value is malformed.
fn integer(fields: &Struct<'_>, id: i16) -> Result<Option<i64>, Error> {
    field(fields, id).map(integer_value).transpose()
}

fn boolean(fields: &Struct<'_>, id: i16) -> Result<Option<bool>, Error> {
    match field(fields, id) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(_) => Err(Error::Malformed),
    }
}

fn bytes<'a>(fields: &Struct<'a>, id: i16) -> Result<Option<&'a [u8]>, Error> {
    match field(fields, id) {
        None => Ok(None),
        Some(Value::Binary(bytes)) => Ok(Some(bytes)),
        Some(_) => Err(Error::Malformed),
    }
}

/// A text field: a binary that must hold UTF-8.
fn text(fields: &Struct<'_>, id: i16) -> Result<Option<String>, Error> {
    bytes(fields, id)?.map(utf8).transpose()
}

fn utf8(bytes: &[u8]) -> Result<String, Error> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| Error::Malformed)
}

fn texts(items: Sequence<'_>) -> Result<Vec<String>, Error> {
    items
        .into_iter()
        .map(|item| match item? {
            Value::Binary(bytes) => utf8(bytes),
            _ => Err(Error::Malformed),
        })
        .collect()
}

fn structure<'a>(fields: &Struct<'a>, id: i16) -> Result<Option<Value<'a>>, Error> {
    match field(fields, id) {
        None => Ok(None),
        Some(value @ Value::Struct(_)) => Ok(Some(value)),
        Some(_) => Err(Error::Malformed),
    }
}

fn list<'a>(fields: &Struct<'a>, id: i16) -> Result<Option<Sequence<'a>>, Error> {
    match field(fields, id) {
        None => Ok(None),
        Some(Value::Sequence(items)) => Ok(Some(items)),
        Some(_) => Err(Error::Malformed),
    }
}

#[cfg(test)]
mod tests {
    //! The footer decoder's seam that the readers cannot reach: footers of
    //! more row groups, or of more nested groups and leaves, than a test
    //! file holds.
    use super::*;

    fn varint(mut value: u64, output: &mut Vec<u8>) {
        while value >= 0x80 {
            output.push(value as u8 | 0x80);
            value >>= 7;
        }
        output.push(value as u8);
    }

    fn zigzag(value: i64) -> Vec<u8> {
        let mut bytes = Vec::new();
        varint(((value << 1) ^ (value >> 63)) as u64, &mut bytes);
        bytes
    }

    /// Append a Compact Protocol list header for `count` elements of `kind`.
    fn header(footer: &mut Vec<u8>, kind: u8, count: usize) {
        if count < 15 {
            footer.push((count as u8) << 4 | kind);
            return;
        }
        footer.push(0xf0 | kind);
        varint(count as u64, footer);
    }

    /// A Compact Protocol struct, written one field at a time.
    #[derive(Default)]
    struct Fields {
        bytes: Vec<u8>,
        last: i16,
    }

    impl Fields {
        fn raw(mut self, id: i16, kind: u8, value: &[u8]) -> Self {
            match id - self.last {
                delta @ 1..=15 => self.bytes.push((delta as u8) << 4 | kind),
                _ => {
                    self.bytes.push(kind);
                    self.bytes.extend(zigzag(id.into()));
                }
            }
            self.last = id;
            self.bytes.extend(value);
            self
        }

        fn integer(self, id: i16, kind: u8, value: i64) -> Self {
            self.raw(id, kind, &zigzag(value))
        }

        fn binary(self, id: i16, value: &[u8]) -> Self {
            let mut bytes = Vec::new();
            varint(value.len() as u64, &mut bytes);
            bytes.extend(value);
            self.raw(id, 8, &bytes)
        }

        fn strct(self, id: i16, value: Fields) -> Self {
            self.raw(id, 12, &value.end())
        }

        fn list(self, id: i16, kind: u8, items: &[Vec<u8>]) -> Self {
            let mut bytes = Vec::new();
            header(&mut bytes, kind, items.len());
            bytes.extend(items.concat());
            self.raw(id, 9, &bytes)
        }

        fn end(mut self) -> Vec<u8> {
            self.bytes.push(0);
            self.bytes
        }
    }

    /// An independently authored footer: one REQUIRED INT64 column `V` in
    /// `groups` one-row row groups. Row group `i` holds `i` in a page at
    /// `page(i)`, with `i` as its statistics and its page indexes at
    /// `INDEXES` past that page. The first `ordinals` row groups hold their
    /// ordinal, as an `i16` that wraps after 32,767. `lists` repeats the
    /// row-group list.
    fn footer(groups: usize, ordinals: usize, lists: usize) -> Vec<u8> {
        let group = |index: usize| {
            let value = (index as i64).to_le_bytes();
            let statistics = Fields::default().integer(3, 6, 0);
            let statistics = statistics.binary(5, &value).binary(6, &value);
            let meta = Fields::default()
                .integer(1, 5, 2) // INT64
                .list(2, 5, &[zigzag(0)]) // encodings: PLAIN
                .list(3, 8, &[b"\x01V".to_vec()]) // path_in_schema
                .integer(4, 5, 0) // UNCOMPRESSED
                .integer(5, 6, 1) // num_values
                .integer(6, 6, 32)
                .integer(7, 6, 32)
                .integer(9, 6, page(index))
                .strct(12, statistics);
            let chunk = Fields::default().integer(2, 6, 0).strct(3, meta);
            // The offset index's and column index's offsets and lengths.
            let chunk = chunk.integer(4, 6, page(index) + INDEXES).integer(5, 5, 16);
            let chunk = chunk
                .integer(6, 6, page(index) + 2 * INDEXES)
                .integer(7, 5, 24);
            let group = Fields::default().list(1, 12, &[chunk.end()]);
            let group = group.integer(2, 6, 32).integer(3, 6, 1);
            if index < ordinals {
                group.integer(7, 4, i64::from(index as i16)).end()
            } else {
                group.end()
            }
        };
        let root = Fields::default()
            .binary(4, b"schema")
            .integer(5, 5, 1)
            .end();
        // INT64; REQUIRED; name.
        let leaf = Fields::default().integer(1, 5, 2).integer(3, 5, 0);
        let leaf = leaf.binary(4, b"V").end();
        let mut footer = Fields::default()
            .integer(1, 5, 1)
            .list(2, 12, &[root, leaf]);
        footer = footer.integer(3, 6, groups as i64);
        let list: Vec<_> = (0..groups).map(group).collect();
        for _ in 0..lists {
            footer = footer.list(4, 12, &list);
        }
        footer.binary(6, b"yamaa test").end()
    }

    /// Where row group `index` places its page.
    fn page(index: usize) -> i64 {
        4 + 32 * index as i64
    }

    /// How far past a row group's page its offset index lies, and its column
    /// index twice as far.
    const INDEXES: i64 = 1 << 32;

    /// Each row group holds its own one row and page.
    fn assert_own_chunks(groups: &[RowGroup], count: usize) {
        assert_eq!(groups.len(), count);
        for (index, group) in groups.iter().enumerate() {
            assert_eq!(group.num_rows, 1, "{index}");
            assert_eq!(group.columns[0].data_page_offset, page(index), "{index}");
        }
    }

    #[test]
    fn list_headers_take_the_short_form_below_fifteen_elements() {
        for (count, expected) in [
            (0, &[0x0c][..]),
            (14, &[0xec]),
            (15, &[0xfc, 15]),
            (32_768, &[0xfc, 0x80, 0x80, 2]),
        ] {
            let mut bytes = Vec::new();
            header(&mut bytes, 12, count);
            assert_eq!(bytes, expected, "{count}");
        }
    }

    #[test]
    fn every_row_group_reads_in_file_order_with_its_own_chunks() {
        // The ordinals each row group holds, or does not, number nothing.
        for ordinals in [0, 3, 8] {
            let footer = decode(&footer(8, ordinals, 1)).unwrap();
            assert_eq!(footer.num_rows, 8);
            assert_own_chunks(&footer.row_groups, 8);
        }
    }

    #[test]
    fn more_row_groups_than_an_i16_ordinal_numbers_read_whole() {
        let count = (1 << 15) + 1;
        // No ordinal, as pyarrow writes; the first 32,768 ordinals, as Polars
        // writes them; and every ordinal, wrapping after 32,767.
        for ordinals in [0, 1 << 15, count] {
            let footer = decode(&footer(count, ordinals, 1)).unwrap();
            assert_eq!(footer.num_rows, count as i64, "{ordinals}");
            assert_own_chunks(&footer.row_groups, count);
        }
    }

    /// A footer of the shape #4782 reports: no row group, `depth` REQUIRED
    /// groups `g` nested below the message root, and `leaves` REQUIRED INT64
    /// leaves `x` in the innermost, or below the root when `depth` is 0.
    fn nested(depth: usize, leaves: usize) -> Vec<u8> {
        let group = |name: &[u8], children: usize| {
            let group = Fields::default().integer(3, 5, 0).binary(4, name);
            group.integer(5, 5, children as i64).end()
        };
        let children = |level: usize| if level == depth { leaves } else { 1 };
        let mut elements = vec![group(b"schema", children(0))];
        elements.extend((1..=depth).map(|level| group(b"g", children(level))));
        let leaf = Fields::default().integer(1, 5, 2).integer(3, 5, 0);
        elements.extend(std::iter::repeat_n(leaf.binary(4, b"x").end(), leaves));
        let footer = Fields::default().integer(1, 5, 1).list(2, 12, &elements);
        let footer = footer.integer(3, 6, 0).list(4, 12, &[]);
        footer.binary(6, b"yamaa test").end()
    }

    /// The Parquet file that holds `footer` and no page.
    fn file(footer: &[u8]) -> Vec<u8> {
        let length = u32::try_from(footer.len()).unwrap().to_le_bytes();
        [b"PAR1", footer, &length[..], b"PAR1"].concat()
    }

    /// Set in the child process that decodes the footer under a bounded
    /// address space. Only Linux starts one.
    #[cfg(target_os = "linux")]
    const CHILD: &str = "YAMAA_PARQUET_FOOTER_CHILD";

    #[test]
    fn a_leaf_below_nested_groups_reads_its_path_from_the_names_above_it() {
        // Under memory overcommit a plain read cannot tell how much it asked
        // for, so on Linux a child process decodes the footer, with its
        // address space bounded to 2 GiB. The child only decodes.
        #[cfg(target_os = "linux")]
        if std::env::var_os(CHILD).is_none() {
            let test = concat!(
                module_path!(),
                "::a_leaf_below_nested_groups_reads_its_path_from_the_names_above_it"
            );
            let output = std::process::Command::new("/bin/sh")
                .args(["-c", "ulimit -v 2097152 && exec \"$0\" \"$@\""])
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", test.split_once("::").unwrap().1])
                .arg("--test-threads=1")
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            // The child ran this test rather than matching nothing.
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("1 passed"), "{stdout}");
            return;
        }
        // The deepest tree admission takes, `SEMANTIC_DEPTH` less the root
        // and the leaf, over leaves enough that a path held by each would
        // copy the 126 names above them 600,000 times: 75,600,000 names,
        // which the bound does not hold, from a footer of 4.8 million bytes
        // (#4782).
        let (depth, leaves) = (126, 600_000);
        let footer = decode(&nested(depth, leaves)).unwrap();
        let schema = &footer.schema;
        // Authored from the tree: 126 groups `g`, then the leaf `x`.
        let path = format!("{}x", "g.".repeat(depth));
        // The nodes are the elements, each holding its own name once.
        assert_eq!(schema.nodes.len(), 1 + depth + leaves);
        assert_eq!(schema.leaves.len(), leaves);
        for (index, leaf) in schema.leaves.iter().enumerate() {
            let levels = (leaf.root, leaf.max_definition, leaf.max_repetition);
            assert_eq!(levels, (0, 0, 0), "{index}");
            assert_eq!(schema.path_bytes(leaf), Some(path.len()), "{index}");
            assert!(schema.named(leaf, &path), "{index}");
        }
        let last = &schema.leaves[leaves - 1];
        assert_eq!(schema.path(last), path);
        for other in [&path[2..], &format!("g.{path}"), &path[..path.len() - 2]] {
            assert!(!schema.named(last, other), "{} bytes", other.len());
        }
    }

    #[test]
    fn a_path_joins_its_names_with_a_period_whatever_the_names_hold() {
        // message { group `a` { `b`, `` }, `a.b`, `` }: a name may hold a
        // period or nothing, so two leaves may share a path.
        let leaf = |name: &[u8]| {
            let leaf = Fields::default().integer(1, 5, 2).integer(3, 5, 0);
            leaf.binary(4, name).end()
        };
        let group = |name: &[u8], children: i64| {
            let group = Fields::default().integer(3, 5, 0).binary(4, name);
            group.integer(5, 5, children).end()
        };
        let elements = [
            Fields::default().binary(4, b"m").integer(5, 5, 3).end(),
            group(b"a", 2),
            leaf(b"b"),
            leaf(b""),
            leaf(b"a.b"),
            leaf(b""),
        ];
        let footer = Fields::default().integer(1, 5, 1).list(2, 12, &elements);
        let footer = footer.integer(3, 6, 0).list(4, 12, &[]).end();
        let schema = decode(&footer).unwrap().schema;
        let paths: Vec<_> = schema
            .leaves
            .iter()
            .map(|leaf| (schema.path(leaf), schema.path_bytes(leaf), leaf.root))
            .collect();
        assert_eq!(
            paths,
            [
                ("a.b".to_owned(), Some(3), 0),
                ("a.".to_owned(), Some(2), 0),
                ("a.b".to_owned(), Some(3), 1),
                (String::new(), Some(0), 2),
            ]
        );
        for (leaf, name, named) in [
            (0, "a.b", true),
            (0, "b", false),
            (0, "a.b.", false),
            (0, "m.a.b", false),
            (1, "a.", true),
            (1, "a", false),
            (1, "", false),
            (2, "a.b", true),
            (2, ".a.b", false),
            (3, "", true),
            (3, ".", false),
        ] {
            let stored = &schema.leaves[leaf];
            assert_eq!(schema.named(stored, name), named, "leaf {leaf}, {name:?}");
        }
    }

    #[test]
    fn a_signature_whose_paths_outgrow_the_file_is_refused_before_one_is_built() {
        use crate::parquet_source::reader::{signature, Error as Refusal};
        // 1,000 leaves, each below 126 groups: 253 bytes of path each, in a
        // file of a few bytes for each element.
        let deep = file(&nested(126, 1_000));
        assert_eq!(
            signature(&deep),
            Err(Refusal::ResourceLimit {
                resource: "source_bytes",
                limit: deep.len(),
                actual: Some(253_000),
            })
        );
        // The same leaves below the root spell the names the footer holds.
        let flat = signature(&file(&nested(0, 1_000))).unwrap();
        assert_eq!(flat.len(), 1_000);
        assert!(flat.iter().all(|column| column.path == "x"));
    }

    #[test]
    fn a_footer_without_a_row_group_list_does_not_decode() {
        // The decoder requires the list; admission refuses a repeated one.
        assert_eq!(decode(&footer(5, 0, 0)).unwrap_err(), Error::Malformed);
        assert_eq!(decode(&footer(5, 0, 1)).unwrap().row_groups.len(), 5);
    }
}
