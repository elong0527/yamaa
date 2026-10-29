//! The interface handshake of the R binding, the gate that every native
//! routine passes, and the census of the binding's live handles
//! (`rust/interface/README.md`, "Version and feature handshake").
//!
//! The generated `.onLoad` calls `interface_handshake` before any other
//! routine. Every other routine replies through `api_entry::reply`, which
//! passes the gate first, so until a negotiation is accepted each one
//! refuses with an empty issues frame.
use crate::api_entry::{answer, reply, Refusal};
use crate::function_callback::text_at;
use extendr_api::prelude::*;
use yamaa_boundary::{Census, EngineFacts, Gate, Outcome, Value, DEPTH};

/// The gate, which the first accepted handshake opens.
static GATE: Gate = Gate::new();

/// The binding's live native handles, by kind.
pub(crate) static CENSUS: Census = Census::new();

/// Refuse a native routine until the interface handshake succeeds.
pub(crate) fn gate() -> std::result::Result<(), Refusal> {
    GATE.pass()
        .map_err(|closed| Refusal::State(closed.to_string()))
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
        let record = match GATE.negotiate(&value(&request, 0), &engine) {
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

/// The live native handles by kind, in the census's order, as a named
/// integer vector: the private accessor of the handle lifetime suites.
#[extendr]
fn live_handles() -> List {
    reply(|| {
        let counts = CENSUS.counts();
        let mut census = r!(counts
            .iter()
            .map(|(_, count)| i32::try_from(*count).unwrap_or(i32::MAX))
            .collect::<Vec<i32>>());
        census
            .set_attrib(
                "names",
                Strings::from_values(counts.iter().map(|(kind, _)| kind.as_str())),
            )
            .map_err(|_| "census names")?;
        Ok(census)
    })
}

/// A request at `depth` below its mapping, as the boundary reads it, never
/// deeper than [`DEPTH`]. A list with names is the request's mapping, where a
/// name R cannot give as text reads as the empty key, which names no field; a
/// list without names above the items is a sequence; and an atomic vector, at
/// any depth, is a vector of its elements. An integer or a whole, finite
/// double is an integer, as R writes integers; text is read strictly, and
/// `NA` text reads as "NA", which no identifier is. A logical, a missing
/// number, a list with names below the request, a list among the items or
/// any other object is no field of a request.
fn value(object: &Robj, depth: usize) -> Value {
    match object.rtype() {
        Rtype::List => {
            let names = object.get_attrib(names_symbol());
            if depth >= DEPTH || (depth > 0 && names.is_some()) {
                return Value::Other;
            }
            let items: Vec<Value> = object
                .as_list()
                .map(|list| list.values().map(|item| value(&item, depth + 1)).collect())
                .unwrap_or_default();
            match names {
                None => Value::List(items),
                Some(names) => Value::Map(
                    (0..names.len())
                        .map(|index| text_at(&names, index).unwrap_or_default())
                        .zip(items)
                        .collect(),
                ),
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
