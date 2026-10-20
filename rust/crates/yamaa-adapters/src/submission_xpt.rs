//! SAS Transport version 5 byte writer over a held published table (XPT-2
//! through XPT-8): the public TS-140 record layout, blank padding and IBM
//! System/360 doubles.
//!
//! yamaa-core owns value and file admission (`submission_transport::admit`,
//! `submission_xpt::admit`) and the typed document (`submission_xpt::document`);
//! this writer owns only the byte projection and repeats no semantic decision.
use yamaa_core::{
    application_issue::{Fault, Stage},
    limits::{LimitExceeded, Limits},
    submission_composition::Finding,
    submission_xpt::{XptDocument, XptKind, RECORD},
    table::{TableAccess, ValueRef},
};

/// Days from 1960-01-01, the SAS date origin, to 1970-01-01.
const SAS_EPOCH_DAYS: i64 = 3653;
/// The SAS release and operating system the first header records name.
const VERSION: &str = "9.4";
const SYSTEM: &str = "yamaa";

/// Text left aligned and blank padded to `width` bytes. Admission already
/// bounded every name, label and value to printable ASCII within its field.
fn padded(text: &str, width: usize) -> Vec<u8> {
    let mut bytes = text.as_bytes()[..text.len().min(width)].to_vec();
    bytes.resize(width, b' ');
    bytes
}
/// A header record: the fixed prefix, the record kind and its counts.
fn header(kind: &str, counts: &str) -> Vec<u8> {
    let mut record =
        format!("HEADER RECORD*******{kind:<8}HEADER RECORD!!!!!!!{counts}").into_bytes();
    record.resize(RECORD, b' ');
    record
}

/// Convert a finite binary64 exactly to an IBM System/360 double: a sign bit,
/// a seven-bit excess-64 hexadecimal exponent and a 56-bit fraction in
/// [1/16, 1). Zero, including negative zero, is eight zero bytes. A magnitude
/// at or above 16^63 or below 16^-65 has no IBM double and returns None.
pub fn ibm(value: f64) -> Option<[u8; 8]> {
    if value == 0.0 {
        return Some([0; 8]);
    }
    if !value.is_finite() {
        return None;
    }
    let bits = value.to_bits();
    let sign = (bits >> 63) as u8;
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let mut significand = bits & ((1 << 52) - 1);
    // value = significand * 2^low, with significand's top bit at 2^(low + 52)
    // for a normal number; a subnormal is below 16^-65 and has no IBM double.
    if biased == 0 {
        return None;
    }
    significand |= 1 << 52;
    let low = biased - 1075;
    let top = low + 52;
    let exponent = top.div_euclid(4) + 1;
    if !(-64..=63).contains(&exponent) {
        return None;
    }
    // fraction = value / 16^exponent * 2^56, an integer in [2^52, 2^56): the
    // shift is top mod 4, so the conversion is exact.
    let fraction = significand << top.rem_euclid(4);
    let mut bytes = [0; 8];
    bytes[0] = (sign << 7) | (exponent + 64) as u8;
    bytes[1..].copy_from_slice(&fraction.to_be_bytes()[1..]);
    Some(bytes)
}

/// One output buffer, checked before every extension against the shared
/// output boundary.
struct Writer<'a> {
    output: Vec<u8>,
    path: &'a str,
}
impl Writer<'_> {
    /// A held cell the writer cannot read or the record layout cannot carry.
    /// Admission already refused every authored one, so this is a byte
    /// projection the writer could not complete.
    fn failure(&self) -> Finding {
        Finding::fault(self.path, Stage::Define, Fault::OutputProjection)
    }
    /// Admit `required` output bytes against the shared output bound; a size
    /// with no representation exceeds it.
    fn admit(&self, required: Option<usize>) -> Result<usize, Finding> {
        required
            .ok_or(LimitExceeded {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual: None,
            })
            .and_then(|required| Limits::output_bytes(required).map(|()| required))
            .map_err(|bound| Finding::limit(self.path, Stage::Define, Fault::OutputLimit, bound))
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), Finding> {
        self.admit(self.output.len().checked_add(bytes.len()))?;
        self.output.extend_from_slice(bytes);
        Ok(())
    }
    /// Blank padding to the next whole record.
    fn pad(&mut self) -> Result<(), Finding> {
        let remainder = self.output.len() % RECORD;
        if remainder > 0 {
            self.bytes(&vec![b' '; RECORD - remainder])?;
        }
        Ok(())
    }
    fn number(&mut self, value: f64) -> Result<(), Finding> {
        let bytes = ibm(value).ok_or_else(|| self.failure())?;
        self.bytes(&bytes)
    }
}

/// Encode an admitted typed document over its held table. Callers run the
/// core admissions over the same table first.
pub fn render<T: TableAccess>(document: &XptDocument, table: &T) -> Result<Vec<u8>, Finding> {
    let size = document.file_bytes(table.row_count());
    let mut writer = Writer {
        output: Vec::new(),
        path: &document.path,
    };
    let capacity = writer.admit(usize::try_from(size).ok())?;
    writer.output.reserve_exact(capacity);
    let created = &document.created;
    // XPT-2, XPT-3: library header, first and second real headers.
    writer.bytes(&header("LIBRARY", &"0".repeat(30)))?;
    let first = format!(
        "{:<8}{:<8}{:<8}{:<8}{:<8}{:24}{created}",
        "SAS", "SAS", "SASLIB", VERSION, SYSTEM, ""
    );
    writer.bytes(&padded(&first, RECORD))?;
    writer.bytes(&padded(created, RECORD))?;
    // Member and descriptor headers, then the two member records.
    writer.bytes(&header("MEMBER", "000000000000000001600000000140"))?;
    writer.bytes(&header("DSCRPTR", &"0".repeat(30)))?;
    let member = format!(
        "{:<8}{:<8}{:<8}{:<8}{:<8}{:24}{created}",
        "SAS", document.name, "SASDATA", VERSION, SYSTEM, ""
    );
    writer.bytes(&padded(&member, RECORD))?;
    let mut second = padded(created, 32);
    second.extend(padded(&document.label, 40));
    second.extend(padded("", 8));
    writer.bytes(&second)?;
    // XPT-4: the NAMESTR header and one 140-byte NAMESTR per variable.
    writer.bytes(&header(
        "NAMESTR",
        &format!("000000{:04}00000000000000000000", document.variables.len()),
    ))?;
    let mut position = 0u32;
    for (index, variable) in document.variables.iter().enumerate() {
        let short = |value: usize| -> Result<[u8; 2], Finding> {
            u16::try_from(value)
                .ok()
                .filter(|value| *value <= 0x7fff)
                .map(u16::to_be_bytes)
                .ok_or_else(|| writer.failure())
        };
        let mut namestr = Vec::with_capacity(140);
        namestr.extend(short(if variable.kind == XptKind::Character {
            2
        } else {
            1
        })?);
        namestr.extend([0, 0]);
        namestr.extend(short(variable.width)?);
        namestr.extend(short(index + 1)?);
        namestr.extend(padded(&variable.name, 8));
        namestr.extend(padded(&variable.label, 40));
        match &variable.format {
            Some(format) => {
                namestr.extend(padded(&format.name, 8));
                namestr.extend(format.width.to_be_bytes());
                namestr.extend(format.decimals.to_be_bytes());
            }
            None => {
                namestr.extend(padded("", 8));
                namestr.extend([0, 0, 0, 0]);
            }
        }
        // Left justification, two fill bytes, then no informat (XPT-9).
        namestr.extend([0, 0, 0, 0]);
        namestr.extend(padded("", 8));
        namestr.extend([0, 0, 0, 0]);
        namestr.extend(position.to_be_bytes());
        namestr.resize(140, 0);
        writer.bytes(&namestr)?;
        position += u32::try_from(variable.width).map_err(|_| writer.failure())?;
    }
    writer.pad()?;
    // XPT-2: the observation header, then each row's cells in column order.
    writer.bytes(&header("OBS", &"0".repeat(30)))?;
    for row in 0..table.row_count() {
        for (column, variable) in document.variables.iter().enumerate() {
            let value = table.cell(row, column).map_err(|_| writer.failure())?;
            match (variable.kind, value) {
                (XptKind::Character, ValueRef::Missing) => {
                    writer.bytes(&padded("", variable.width))?
                }
                (XptKind::Character, ValueRef::Str(text)) if text.len() <= variable.width => {
                    writer.bytes(&padded(text, variable.width))?
                }
                (XptKind::Character, ValueRef::Date(date)) => {
                    writer.bytes(&padded(&date.to_string(), variable.width))?
                }
                (XptKind::Character, ValueRef::DateTime(moment)) => {
                    writer.bytes(&padded(&moment.to_string(), variable.width))?
                }
                (_, ValueRef::Missing) => writer.bytes(&[b'.', 0, 0, 0, 0, 0, 0, 0])?,
                (XptKind::Number, ValueRef::Int(value)) => writer.number(value as f64)?,
                (XptKind::Number, ValueRef::Float(value)) => writer.number(value.get())?,
                (XptKind::Date, ValueRef::Date(date)) => writer.number(
                    (i64::from(crate::arrow_temporal::date_days(date)) + SAS_EPOCH_DAYS) as f64,
                )?,
                (XptKind::DateTime, ValueRef::DateTime(moment)) => writer.number(
                    (crate::arrow_temporal::datetime_seconds(moment) + SAS_EPOCH_DAYS * 86_400)
                        as f64,
                )?,
                _ => return Err(writer.failure()),
            }
        }
    }
    writer.pad()?;
    Ok(writer.output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02X}")).collect()
    }

    /// Independently derived IBM doubles: sign, excess-64 hexadecimal exponent
    /// and the binary64 significand shifted into a normalized 56-bit fraction.
    #[test]
    fn ibm_doubles_are_exact_and_normalized() {
        for (value, expected) in [
            (1.0, "4110000000000000"),
            (-1.0, "C110000000000000"),
            (0.5, "4080000000000000"),
            (100.0, "4264000000000000"),
            (0.1, "401999999999999A"),
            (-118.625, "C276A00000000000"),
            (3653.0, "43E4500000000000"),
            (f64::powi(2.0, 53), "4E20000000000000"),
            (f64::powi(2.0, -260), "0010000000000000"),
            (f64::MIN_POSITIVE * f64::powi(2.0, 762), "0010000000000000"),
        ] {
            assert_eq!(hex(ibm(value).unwrap()), expected, "{value}");
        }
        assert_eq!(ibm(0.0), Some([0; 8]));
        assert_eq!(ibm(-0.0), Some([0; 8]));
        assert_eq!(ibm(f64::powi(2.0, 252)), None);
        assert_eq!(ibm(f64::powi(2.0, -261)), None);
        assert_eq!(ibm(f64::MIN_POSITIVE / 2.0), None);
        let largest = f64::powi(2.0, 252) * (1.0 - f64::EPSILON / 2.0);
        assert_eq!(hex(ibm(largest).unwrap()), "7FFFFFFFFFFFFFF8");
    }

    /// A size past the output bound, or with no representation, is the bound;
    /// a held number with no IBM double is a projection fault.
    #[test]
    fn refusals_name_the_output_bound_or_a_projection_fault() {
        use yamaa_core::{diagnostic::ConditionCode, json::Value as V};
        let mut writer = Writer {
            output: Vec::new(),
            path: "datasets.DM.transport",
        };
        let text = |value: &str| V::String(value.into());
        assert_eq!(
            writer.admit(Some(Limits::OUTPUT_BYTES)),
            Ok(Limits::OUTPUT_BYTES)
        );
        for (required, actual) in [
            (
                Some(Limits::OUTPUT_BYTES + 1),
                Some(Limits::OUTPUT_BYTES + 1),
            ),
            (None, None),
        ] {
            let finding = writer.admit(required).unwrap_err();
            assert_eq!(finding.code, ConditionCode::ApplicationRejected);
            assert_eq!(finding.path, "datasets.DM.transport");
            let mut expected: yamaa_core::diagnostic::Context = [
                ("code".into(), text("output_limit")),
                ("limit".into(), V::from(Limits::OUTPUT_BYTES)),
                ("resource".into(), text("output_bytes")),
                ("stage".into(), text("define")),
            ]
            .into();
            if let Some(actual) = actual {
                expected.insert("actual".into(), V::from(actual));
            }
            assert_eq!(finding.context, expected);
        }
        let finding = writer.number(f64::MAX).unwrap_err();
        assert_eq!(
            finding.context,
            [
                ("code".into(), text("output_projection")),
                ("stage".into(), text("define")),
            ]
            .into()
        );
        assert!(writer.output.is_empty());
    }
}
