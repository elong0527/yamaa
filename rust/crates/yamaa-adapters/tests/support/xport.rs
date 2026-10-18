//! SAS Transport files assembled at test time from the documented 80-byte
//! records and 140-byte NAMESTR layout, as `tools/staging/build_xport_fixtures.py`
//! writes the committed transport fixtures. Fields here are character fields,
//! so no IBM numbers are encoded, and nothing reads the result with yamaa.

/// One 80-byte record, padded with blanks.
pub fn record(text: &[u8]) -> Vec<u8> {
    assert!(text.len() <= 80);
    let mut bytes = text.to_vec();
    bytes.resize(80, b' ');
    bytes
}

/// A header record: the fixed prefixes around an eight-byte name, then `tail`
/// from byte 48.
pub fn header(name: &str, tail: &str) -> Vec<u8> {
    let mut text = b"HEADER RECORD*******".to_vec();
    text.extend(format!("{name:<8}").as_bytes());
    text.extend(b"HEADER RECORD!!!!!!!");
    text.extend(tail.as_bytes());
    record(&text)
}

/// A one-member transport file of character fields `(name, width)`. Version
/// 8 names its records `LIBV8`, `MEMBV8`, `DSCPTV8`, `NAMSTV8` and `OBSV8`,
/// and stores each field name in the 32-byte long name. `observations` is
/// the observation header's text from byte 48, and `data` the records,
/// padded with blanks to a whole 80-byte record.
pub fn transport(version: u8, fields: &[(&str, u16)], observations: &str, data: &[u8]) -> Vec<u8> {
    let names = if version == 5 {
        ["LIBRARY", "MEMBER", "DSCRPTR", "NAMESTR", "OBS"]
    } else {
        ["LIBV8", "MEMBV8", "DSCPTV8", "NAMSTV8", "OBSV8"]
    };
    let zeros = "0".repeat(30);
    let mut bytes = header(names[0], &zeros);
    bytes.extend(record(b"SAS     SAS     SASLIB  6.06    fixture"));
    bytes.extend(record(b"01JAN60:00:00:00"));
    bytes.extend(header(names[1], &zeros));
    bytes.extend(header(names[2], &zeros));
    bytes.extend(record(b"SAS     BUILT   "));
    bytes.extend(record(b"01JAN60:00:00:00"));
    bytes.extend(header(
        names[3],
        &format!("{:05}{:05}{}", 0, fields.len(), "0".repeat(20)),
    ));
    let mut descriptors = Vec::new();
    let mut position = 0u32;
    for (index, &(name, width)) in fields.iter().enumerate() {
        let mut descriptor = [0u8; 140];
        descriptor[0..2].copy_from_slice(&2u16.to_be_bytes());
        descriptor[4..6].copy_from_slice(&width.to_be_bytes());
        descriptor[6..8].copy_from_slice(&(index as u16 + 1).to_be_bytes());
        descriptor[8..16].copy_from_slice(format!("{name:<8}").as_bytes());
        descriptor[16..56].fill(b' ');
        descriptor[56..64].fill(b' ');
        descriptor[72..80].fill(b' ');
        descriptor[84..88].copy_from_slice(&position.to_be_bytes());
        if version == 8 {
            descriptor[88..120].copy_from_slice(format!("{name:<32}").as_bytes());
        }
        descriptors.extend(descriptor);
        position += u32::from(width);
    }
    descriptors.resize(descriptors.len().div_ceil(80) * 80, b' ');
    bytes.extend(descriptors);
    bytes.extend(header(names[4], observations));
    let mut data = data.to_vec();
    data.resize(data.len().div_ceil(80) * 80, b' ');
    bytes.extend(data);
    bytes
}

/// Character records laid out field by field, each value padded with blanks.
pub fn records(fields: &[(&str, u16)], rows: &[&[&str]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in rows {
        for (value, &(_, width)) in row.iter().zip(fields) {
            let mut cell = value.as_bytes().to_vec();
            cell.resize(usize::from(width), b' ');
            bytes.extend(cell);
        }
    }
    bytes
}
