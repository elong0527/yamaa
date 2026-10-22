//! The interface handshake of the R binding and the gate that every native
//! routine passes (`rust/interface/README.md`, "Version and feature
//! handshake").
//!
//! The generated `.onLoad` calls `interface_handshake` before any other
//! routine. Every other routine replies through `api_entry::reply`, which
//! passes the gate first, so until a negotiation is accepted each one
//! refuses with no issues.
use crate::api_entry::answer;
use crate::function_callback::text_at;
use extendr_api::prelude::*;
use yamaa_boundary::{EngineFacts, Gate, Outcome, Value};

/// The gate, which the first accepted handshake opens.
static GATE: Gate = Gate::new();

/// Refuse a native routine until the interface handshake succeeds.
pub(crate) fn gate() -> std::result::Result<(), String> {
    GATE.pass().map_err(|closed| closed.to_string())
}

/// A boundary version as R holds it: an integer, or a double beyond R's
/// integers, as R reads such a number.
fn version(version: u32) -> Robj {
    i32::try_from(version).map_or_else(|_| r!(f64::from(version)), |version| r!(version))
}

/// Answer an interface request: the accepted record, or the refusal
/// envelope, with `NULL` for an absent version. The first acceptance opens
/// the gate; a later call answers its request and changes nothing.
#[extendr]
fn interface_handshake(request: Robj) -> List {
    answer(|| {
        // The engine information names no host version, which nothing here
        // reads.
        let info = yamaa::info(yamaa::Host::R, None);
        let engine = EngineFacts::new(&info.features, yamaa::VERSION, &info.language_versions);
        let absent = |value: Option<u32>| value.map_or_else(|| NULL.into_robj(), version);
        let record = match GATE.negotiate(&value(&request), &engine) {
            Outcome::Accepted(accepted) => list!(
                accepted = true,
                version = version(accepted.version),
                enabled = Strings::from_values(accepted.enabled),
                supported = Strings::from_values(accepted.supported),
                core_version = accepted.core_version,
                language_versions = Strings::from_values(accepted.language_versions)
            ),
            Outcome::Refused(refusal) => list!(
                accepted = false,
                requested_version = absent(refusal.requested_version),
                supported_version = absent(refusal.supported_version),
                reason = refusal.reason.as_str(),
                missing_features = Strings::from_values(refusal.missing_features)
            ),
        };
        Ok(record.into_robj())
    })
}

/// A request as the boundary reads it: a list with names is a mapping and
/// one without a sequence, and an atomic vector is a vector of its elements.
/// An integer or a whole, finite double is an integer, as R writes integers;
/// text is read strictly, and `NA` text reads as "NA", which no identifier
/// is; a logical, a missing number or any other object is no field of a
/// request.
fn value(object: &Robj) -> Value {
    match object.rtype() {
        Rtype::List => {
            let items: Vec<Value> = object
                .as_list()
                .map(|list| list.values().map(|item| value(&item)).collect())
                .unwrap_or_default();
            match object.get_attrib(names_symbol()) {
                None => Value::List(items),
                Some(names) => (0..names.len())
                    .map(|index| text_at(&names, index).ok())
                    .collect::<Option<Vec<String>>>()
                    .map_or(Value::Other, |names| {
                        Value::Map(names.into_iter().zip(items).collect())
                    }),
            }
        }
        Rtype::Integers => object.as_integer_slice().map_or(Value::Other, |values| {
            Value::Vector(
                values
                    .iter()
                    .map(|value| {
                        if value.is_na() {
                            Value::Other
                        } else {
                            Value::Integer(i64::from(*value))
                        }
                    })
                    .collect(),
            )
        }),
        Rtype::Doubles => object.as_real_slice().map_or(Value::Other, |values| {
            Value::Vector(values.iter().map(|value| whole(*value)).collect())
        }),
        Rtype::Strings => Value::Vector(
            (0..object.len())
                .map(|index| text_at(object, index).map_or(Value::Other, Value::Text))
                .collect(),
        ),
        Rtype::Logicals => Value::Vector(vec![Value::Other; object.len()]),
        _ => Value::Other,
    }
}

/// A double as an integer when it is whole, finite and within 64 bits.
fn whole(value: f64) -> Value {
    // 2^63, the first magnitude beyond `i64`.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if value.is_finite() && value.fract() == 0.0 && value.abs() < LIMIT {
        Value::Integer(value as i64)
    } else {
        Value::Other
    }
}

include!("generated/interface.rs");
