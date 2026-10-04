use yamaa_adapters::scalar_transport::{scalar_round_trip, TransportError, MAX_REQUEST_BYTES};

/// Replay independent wire truth, including precise bits and malformed duplicate fields.
#[test]
fn shared_transport_vectors() {
    for line in include_str!("fixtures/scalar_transport.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3);
        let actual = match scalar_round_trip(fields[1]) {
            Ok(value) => value,
            Err(error) => format!("error:{error}"),
        };
        assert_eq!(actual, fields[2], "{}", fields[0]);
    }
}

/// Bound bytes, not characters, before parsing and retain owned results across calls.
#[test]
fn byte_limit_and_owned_results() {
    let prefix = r#"{"protocol":"scalar/1","value":{"str":""#;
    let suffix = "\"}}";
    let request = format!(
        "{prefix}{}{suffix}",
        "x".repeat(MAX_REQUEST_BYTES - prefix.len() - suffix.len())
    );
    let result = scalar_round_trip(&request).unwrap();
    assert_eq!(result, request);
    assert_eq!(
        scalar_round_trip(&format!("{request} ")),
        Err(TransportError::RequestLimit)
    );
    assert_eq!(
        scalar_round_trip(&"\u{e9}".repeat(MAX_REQUEST_BYTES / 2 + 1)),
        Err(TransportError::RequestLimit)
    );
    assert!(scalar_round_trip("{").is_err());
    assert_eq!(result, request);
    assert_eq!(scalar_round_trip(&request).unwrap(), result);
}
