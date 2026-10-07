//! Storage schema compatibility where Arrow Rust and C++ restore hints differently.
use arrow_schema::{DataType, Field, Fields, Schema};
use std::sync::Arc;

/// A stored date is Date32 even if an original Arrow writer used Date64.
/// Timestamp timezone hints survive writer coercion from seconds to milliseconds;
/// the stored timestamp unit remains authoritative. Neither changes admission of
/// the closed physical/logical pairs, which the profile checks separately.
pub(super) fn normalize(schema: &Schema, hint: Option<&Schema>) -> Schema {
    Schema::new(fields(schema.fields(), hint.map(Schema::fields)))
}

fn fields(stored: &Fields, hint: Option<&Fields>) -> Fields {
    stored
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let hint = hint
                .and_then(|fields| fields.get(index))
                .filter(|hint| hint.name() == field.name());
            Arc::new(normalize_field(field, hint.map(AsRef::as_ref)))
        })
        .collect()
}

fn normalize_field(field: &Field, hint: Option<&Field>) -> Field {
    let data_type = match field.data_type() {
        DataType::Date64 => DataType::Date32,
        DataType::Timestamp(unit, zone) => {
            let zone = match hint.map(Field::data_type) {
                Some(DataType::Timestamp(_, Some(zone))) => Some(zone.clone()),
                _ => zone.clone(),
            };
            DataType::Timestamp(*unit, zone)
        }
        DataType::Struct(children) => {
            let hints = match hint.map(Field::data_type) {
                Some(DataType::Struct(children)) => Some(children),
                _ => None,
            };
            DataType::Struct(fields(children, hints))
        }
        DataType::List(child) | DataType::LargeList(child) | DataType::FixedSizeList(child, _) => {
            let child_hint = match hint.map(Field::data_type) {
                Some(
                    DataType::List(child)
                    | DataType::LargeList(child)
                    | DataType::FixedSizeList(child, _),
                ) => Some(child.as_ref()),
                _ => None,
            };
            let child = Arc::new(normalize_field(child, child_hint));
            match field.data_type() {
                DataType::List(_) => DataType::List(child),
                DataType::LargeList(_) => DataType::LargeList(child),
                DataType::FixedSizeList(_, size) => DataType::FixedSizeList(child, *size),
                _ => unreachable!(),
            }
        }
        DataType::Map(child, sorted) => {
            let child_hint = match hint.map(Field::data_type) {
                Some(DataType::Map(child, _)) => Some(child.as_ref()),
                _ => None,
            };
            DataType::Map(Arc::new(normalize_field(child, child_hint)), *sorted)
        }
        other => other.clone(),
    };
    field.clone().with_data_type(data_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_schema::TimeUnit;
    #[test]
    fn stored_date_unit_and_timestamp_unit_remain_authoritative() {
        let stored = Schema::new(vec![
            Field::new("D", DataType::Date64, true),
            Field::new(
                "T",
                DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
                true,
            ),
        ]);
        let hint = Schema::new(vec![
            Field::new("D", DataType::Date64, true),
            Field::new(
                "T",
                DataType::Timestamp(TimeUnit::Second, Some("America/New_York".into())),
                true,
            ),
        ]);
        let actual = normalize(&stored, Some(&hint));
        assert_eq!(actual.field(0).data_type(), &DataType::Date32);
        assert_eq!(
            actual.field(1).data_type(),
            &DataType::Timestamp(TimeUnit::Millisecond, Some("America/New_York".into()))
        );
        let unrelated = Schema::new(vec![hint.field(1).clone(), hint.field(0).clone()]);
        assert_eq!(
            normalize(&stored, Some(&unrelated)).field(1).data_type(),
            stored.field(1).data_type()
        );
    }
}
