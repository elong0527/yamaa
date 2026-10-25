//! The Arrow schema of an admitted footer (#3878): its schema tree read as
//! parquet 55.2.0's Arrow schema conversion read it, with the hints of the
//! file's embedded Arrow schema, so that each field takes the type, and each
//! file the refusal, that conversion gave it.
//!
//! Each leaf takes the Arrow type its physical type and annotations give it,
//! or the hint's type where the hint names a layout of that type, such as a
//! large string or a dictionary of it (`hinted`). A group is a list, a map
//! or a struct by its converted type, with the levels of nesting the format
//! allows each; a repeated field outside a list is a list of it. A group
//! that holds no field holds no Arrow field either. The hint must have the
//! shape of the schema where it is given: a struct of as many fields, named
//! as the schema's, a list or a map.
use super::footer::{Kind, Node, Repetition, Schema};
use super::format::{Converted, Logical, Physical, TimeUnit as Unit};
use super::reader::Error;
use arrow_schema::{DataType, Field, Fields, IntervalUnit, TimeUnit};
use std::sync::Arc;

/// A converted field: its Arrow type and nullability.
struct Typed {
    data_type: DataType,
    nullable: bool,
}

/// The Arrow schema of `schema`, with the fields of the embedded schema
/// `hint` as hints.
pub(super) fn schema(
    schema: &Schema,
    hint: Option<&arrow_schema::Schema>,
) -> Result<arrow_schema::Schema, Error> {
    let hint = hint.map(|hint| DataType::Struct(hint.fields().clone()));
    let fields = match dispatch(schema, 0, hint.as_ref())? {
        None => Fields::empty(),
        Some(Typed {
            data_type: DataType::Struct(fields),
            ..
        }) => fields,
        // The conversion takes the message root for a struct.
        Some(_) => return Err(Error::Malformed),
    };
    Ok(arrow_schema::Schema::new(fields))
}

/// A node's repetition; the message root's is required.
fn repetition(node: &Node) -> Repetition {
    node.repetition.unwrap_or(Repetition::Required)
}

fn children(node: &Node) -> &[usize] {
    match &node.kind {
        Kind::Group(children) => children,
        Kind::Primitive(_) => &[],
    }
}

fn dispatch(schema: &Schema, at: usize, hint: Option<&DataType>) -> Result<Option<Typed>, Error> {
    let node = &schema.nodes[at];
    match node.kind {
        Kind::Primitive(_) => primitive_field(schema, at, hint),
        Kind::Group(_) => match node.converted {
            Converted::List => list(schema, at, hint),
            Converted::Map | Converted::MapKeyValue => map(schema, at, hint),
            _ => structure(schema, at, hint),
        },
    }
}

/// A repeated field outside a list: a list of it, which is never null.
fn into_list(name: &str, field: Typed) -> Typed {
    Typed {
        data_type: DataType::List(Arc::new(Field::new(name, field.data_type, false))),
        nullable: false,
    }
}

/// The Arrow field of node `node`, converted as `field`, with the hint
/// field `hint`: a dictionary keeps the hint's ordering.
fn field(node: &Node, field: Typed, hint: Option<&Field>) -> Field {
    let ordered = match (&field.data_type, hint.and_then(Field::dict_is_ordered)) {
        (DataType::Dictionary(..), Some(ordered)) => ordered,
        _ => false,
    };
    Field::new(node.name.clone(), field.data_type, field.nullable).with_dict_is_ordered(ordered)
}

fn primitive_field(
    schema: &Schema,
    at: usize,
    hint: Option<&DataType>,
) -> Result<Option<Typed>, Error> {
    let node = &schema.nodes[at];
    let Kind::Primitive(leaf) = node.kind else {
        return Err(Error::Malformed);
    };
    let repetition = repetition(node);
    let parquet = primitive(&schema.leaves[leaf])?;
    let data_type = match hint {
        Some(hint) => hinted(parquet, hint.clone()),
        None => parquet,
    };
    let converted = Typed {
        data_type,
        nullable: repetition == Repetition::Optional,
    };
    Ok(Some(match repetition {
        Repetition::Repeated => into_list(&node.name, converted),
        _ => converted,
    }))
}

fn structure(schema: &Schema, at: usize, hint: Option<&DataType>) -> Result<Option<Typed>, Error> {
    let node = &schema.nodes[at];
    let repetition = repetition(node);
    let children = children(node);
    let hints = match hint {
        Some(DataType::Struct(fields)) if fields.len() == children.len() => Some(fields),
        Some(_) => return Err(Error::Malformed),
        None => None,
    };
    let mut fields = Vec::with_capacity(children.len());
    for (index, &child) in children.iter().enumerate() {
        let hint = hints.map(|fields| fields[index].as_ref());
        if hint.is_some_and(|hint| *hint.name() != schema.nodes[child].name) {
            return Err(Error::Malformed);
        }
        if let Some(converted) = dispatch(schema, child, hint.map(Field::data_type))? {
            fields.push(field(&schema.nodes[child], converted, hint));
        }
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let converted = Typed {
        data_type: DataType::Struct(fields.into()),
        nullable: repetition == Repetition::Optional,
    };
    Ok(Some(match repetition {
        Repetition::Repeated => into_list(&node.name, converted),
        _ => converted,
    }))
}

fn map(schema: &Schema, at: usize, hint: Option<&DataType>) -> Result<Option<Typed>, Error> {
    let node = &schema.nodes[at];
    let nullable = match repetition(node) {
        Repetition::Required => false,
        Repetition::Optional => true,
        Repetition::Repeated => return Err(Error::Malformed),
    };
    let [entries] = children(node) else {
        return Err(Error::Malformed);
    };
    let entries = &schema.nodes[*entries];
    if entries.repetition != Some(Repetition::Repeated) {
        return Err(Error::Malformed);
    }
    // A map whose entries hold one field is read as a list of them.
    let &[key, value] = children(entries) else {
        return match children(entries).len() {
            1 => list(schema, at, hint),
            _ => Err(Error::Malformed),
        };
    };
    let (key_node, value_node) = (&schema.nodes[key], &schema.nodes[value]);
    if [key_node, value_node]
        .iter()
        .any(|node| node.repetition == Some(Repetition::Repeated))
    {
        return Err(Error::Malformed);
    }
    let (key_hint, value_hint, sorted) = match hint {
        Some(DataType::Map(entries, sorted)) => match entries.data_type() {
            DataType::Struct(fields) if fields.len() == 2 => {
                (Some(fields[0].as_ref()), Some(fields[1].as_ref()), *sorted)
            }
            _ => return Err(Error::Malformed),
        },
        Some(_) => return Err(Error::Malformed),
        None => (None, None, false),
    };
    let key = dispatch(schema, key, key_hint.map(Field::data_type))?;
    let value = dispatch(schema, value, value_hint.map(Field::data_type))?;
    let (Some(key), Some(value)) = (key, value) else {
        return Ok(None);
    };
    let key = field(key_node, key, key_hint).with_nullable(false);
    let value = field(value_node, value, value_hint);
    // The entries are never null.
    let entries = Field::new_struct(
        entries.name.clone(),
        [Arc::new(key), Arc::new(value)],
        false,
    );
    Ok(Some(Typed {
        data_type: DataType::Map(Arc::new(entries), sorted),
        nullable,
    }))
}

fn list(schema: &Schema, at: usize, hint: Option<&DataType>) -> Result<Option<Typed>, Error> {
    let node = &schema.nodes[at];
    if matches!(node.kind, Kind::Primitive(_)) {
        return Err(Error::Malformed);
    }
    let &[repeated] = children(node) else {
        return Err(Error::Malformed);
    };
    let repeated_node = &schema.nodes[repeated];
    if repetition(repeated_node) != Repetition::Repeated {
        return Err(Error::Malformed);
    }
    // The message root has no repetition, which a list must have.
    let nullable = match node.repetition.ok_or(Error::Malformed)? {
        Repetition::Required => false,
        Repetition::Optional => true,
        Repetition::Repeated => return Err(Error::Malformed),
    };
    let item_hint = match hint {
        Some(
            DataType::List(item) | DataType::LargeList(item) | DataType::FixedSizeList(item, _),
        ) => Some(item.as_ref()),
        Some(_) => return Err(Error::Malformed),
        None => None,
    };
    let nullable_as = |converted: Option<Typed>| {
        converted.map(|converted| Typed {
            nullable,
            ..converted
        })
    };
    if matches!(repeated_node.kind, Kind::Primitive(_)) {
        return Ok(nullable_as(primitive_field(
            schema,
            repeated,
            item_hint.map(Field::data_type),
        )?));
    }
    let items = children(repeated_node);
    // A repeated group of several fields, or one the format's legacy names
    // mark as the element itself, is the list's element.
    let legacy =
        repeated_node.name == "array" || repeated_node.name == format!("{}_tuple", node.name);
    if items.len() != 1
        || (!is_list(repeated_node) && !single_repeated_child(schema, repeated_node) && legacy)
    {
        return Ok(nullable_as(structure(
            schema,
            repeated,
            item_hint.map(Field::data_type),
        )?));
    }
    let item = items[0];
    let Some(converted) = dispatch(schema, item, item_hint.map(Field::data_type))? else {
        return Ok(None);
    };
    let item = Arc::new(field(&schema.nodes[item], converted, item_hint));
    let data_type = match hint {
        Some(DataType::LargeList(_)) => DataType::LargeList(item),
        Some(DataType::FixedSizeList(_, size)) => DataType::FixedSizeList(item, *size),
        _ => DataType::List(item),
    };
    Ok(Some(Typed {
        data_type,
        nullable,
    }))
}

/// Whether a group is annotated as a list: by its logical type when it has
/// one, or else by its converted type.
fn is_list(node: &Node) -> bool {
    match node.logical {
        Some(logical) => logical == Logical::List,
        None => node.converted == Converted::List,
    }
}

/// Whether a group holds exactly one field, a repeated one.
fn single_repeated_child(schema: &Schema, node: &Node) -> bool {
    matches!(children(node), [child] if schema.nodes[*child].repetition == Some(Repetition::Repeated))
}

/// A leaf's Arrow type, from its physical type and annotations.
fn primitive(leaf: &super::footer::Leaf) -> Result<DataType, Error> {
    use Converted as C;
    let time = |unit: Unit| match unit {
        Unit::Millis => TimeUnit::Millisecond,
        Unit::Micros => TimeUnit::Microsecond,
        Unit::Nanos => TimeUnit::Nanosecond,
    };
    let utc = Some(Arc::<str>::from("UTC"));
    let (logical, converted) = (leaf.logical, leaf.converted);
    Ok(match leaf.physical {
        Physical::Boolean => DataType::Boolean,
        Physical::Int96 => DataType::Timestamp(TimeUnit::Nanosecond, None),
        Physical::Float => DataType::Float32,
        Physical::Double => DataType::Float64,
        Physical::Int32 => match (logical, converted) {
            (None, C::None | C::Int32) => DataType::Int32,
            (Some(Logical::Integer { bits, signed }), _) => match (bits, signed) {
                (8, true) => DataType::Int8,
                (16, true) => DataType::Int16,
                (32, true) => DataType::Int32,
                (8, false) => DataType::UInt8,
                (16, false) => DataType::UInt16,
                (32, false) => DataType::UInt32,
                _ => return Err(Error::Malformed),
            },
            (Some(Logical::Decimal { scale, precision }), _) => decimal128(scale, precision)?,
            (Some(Logical::Date), _) | (None, C::Date) => DataType::Date32,
            (
                Some(Logical::Time {
                    unit: Unit::Millis, ..
                }),
                _,
            )
            | (None, C::TimeMillis) => DataType::Time32(TimeUnit::Millisecond),
            (Some(Logical::Unknown), _) => DataType::Null,
            (None, C::Uint8) => DataType::UInt8,
            (None, C::Uint16) => DataType::UInt16,
            (None, C::Uint32) => DataType::UInt32,
            (None, C::Int8) => DataType::Int8,
            (None, C::Int16) => DataType::Int16,
            (None, C::Decimal) => decimal128(leaf.scale, leaf.precision)?,
            _ => return Err(Error::Malformed),
        },
        Physical::Int64 => match (logical, converted) {
            (None, C::None | C::Int64) => DataType::Int64,
            (Some(Logical::Integer { bits: 64, signed }), _) => {
                if signed {
                    DataType::Int64
                } else {
                    DataType::UInt64
                }
            }
            (
                Some(Logical::Time {
                    unit: Unit::Millis, ..
                }),
                _,
            ) => return Err(Error::Malformed),
            (Some(Logical::Time { unit, .. }), _) => DataType::Time64(time(unit)),
            (
                Some(Logical::Timestamp {
                    utc: adjusted,
                    unit,
                }),
                _,
            ) => DataType::Timestamp(time(unit), adjusted.then(|| utc.clone()).flatten()),
            (None, C::Uint64) => DataType::UInt64,
            (None, C::TimeMicros) => DataType::Time64(TimeUnit::Microsecond),
            (None, C::TimestampMillis) => DataType::Timestamp(TimeUnit::Millisecond, utc),
            (None, C::TimestampMicros) => DataType::Timestamp(TimeUnit::Microsecond, utc),
            (Some(Logical::Decimal { scale, precision }), _) => decimal128(scale, precision)?,
            (None, C::Decimal) => decimal128(leaf.scale, leaf.precision)?,
            _ => return Err(Error::Malformed),
        },
        Physical::ByteArray => match (logical, converted) {
            (Some(Logical::String | Logical::Json), _) | (None, C::Json | C::Utf8) => {
                DataType::Utf8
            }
            (Some(Logical::Bson | Logical::Enum), _) | (None, C::None | C::Bson | C::Enum) => {
                DataType::Binary
            }
            (Some(Logical::Decimal { scale, precision }), _) => {
                decimal(scale, precision, precision <= 38)?
            }
            (None, C::Decimal) => decimal(leaf.scale, leaf.precision, leaf.precision <= 38)?,
            _ => return Err(Error::Malformed),
        },
        Physical::FixedLenByteArray => {
            let narrow = leaf.type_length <= 16;
            match (logical, converted) {
                (Some(Logical::Decimal { scale, precision }), _) => {
                    decimal(scale, precision, narrow)?
                }
                (None, C::Decimal) => decimal(leaf.scale, leaf.precision, narrow)?,
                (None, C::Interval) => DataType::Interval(IntervalUnit::DayTime),
                (Some(Logical::Float16), _) if leaf.type_length == 2 => DataType::Float16,
                (Some(Logical::Float16), _) => return Err(Error::Malformed),
                _ => DataType::FixedSizeBinary(leaf.type_length),
            }
        }
    })
}

/// A decimal of `precision` and `scale`, in 128 bits when `narrow` and in
/// 256 otherwise; neither may be out of Arrow's range for it.
fn decimal(scale: i32, precision: i32, narrow: bool) -> Result<DataType, Error> {
    let scale = i8::try_from(scale).map_err(|_| Error::Malformed)?;
    let precision = u8::try_from(precision).map_err(|_| Error::Malformed)?;
    Ok(if narrow {
        DataType::Decimal128(precision, scale)
    } else {
        DataType::Decimal256(precision, scale)
    })
}

fn decimal128(scale: i32, precision: i32) -> Result<DataType, Error> {
    decimal(scale, precision, true)
}

/// The type a leaf of Arrow type `parquet` reads as with the hint's type
/// `hint`: the hint where it names another layout or unit of the same
/// values, a dictionary of the type a dictionary's value type gives, and
/// otherwise `parquet`.
fn hinted(parquet: DataType, hint: DataType) -> DataType {
    use DataType as D;
    use TimeUnit::{Microsecond, Millisecond, Nanosecond, Second};
    match (&parquet, &hint) {
        (D::Int32 | D::Int64, D::Timestamp(..))
        | (D::Int32, D::Time32(_))
        | (D::Int64, D::Time64(_) | D::Duration(_) | D::Date64)
        | (D::Date32, D::Date64)
        | (D::Timestamp(Nanosecond, None), D::Timestamp(Second | Millisecond | Microsecond, _))
        | (D::Utf8, D::LargeUtf8 | D::Utf8View)
        | (D::Binary, D::LargeBinary | D::Utf8 | D::LargeUtf8 | D::Utf8View | D::BinaryView)
        | (D::Interval(_), D::Interval(_))
        | (D::Decimal128(..), D::Decimal256(..)) => hint,
        (D::Timestamp(stored, _), D::Timestamp(unit, Some(_))) if stored == unit => hint,
        (_, D::Dictionary(_, value)) => {
            let value = value.as_ref().clone();
            let hinted = self::hinted(parquet, value.clone());
            if hinted == value {
                hint
            } else {
                hinted
            }
        }
        _ => parquet,
    }
}
