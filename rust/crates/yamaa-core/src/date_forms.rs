//! The closed list of date forms that `date_text` reads (TIME-22). Each form
//! is one shape of the whole text, its numeric fields with the width and
//! range of VALUE-15, and it reads a text into the collected date text of
//! TIME-7 that the text denotes. Only `mdy_slash` and `dmy_slash` share a
//! shape, read in two orders of day and month, and a declared list names at
//! most one of them, so at most one of its forms reads any text.

use crate::diagnostic::{ConditionCode, Diagnostic};
pub use crate::expression_table::DateForm;
use crate::expression_table::Verb;
use crate::json::Value;
use crate::temporal::{Date, Precision};

/// The month abbreviations, January first, each matched in any letter case.
const MONTHS: [&[u8; 3]; 12] = [
    b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
];

/// What each date form reads; the forms themselves are the schema type
/// `date_form`'s values (`crate::expression_table`).
impl DateForm {
    /// The collected date text `text` denotes when the whole of it has this
    /// form's shape and each field lies in its range: `iso` keeps its text,
    /// `monyyyy` gives `YYYY-MM` and every other form `YYYY-MM-DD`. Nothing
    /// is trimmed.
    pub fn read(self, text: &str) -> Option<String> {
        let bytes = text.as_bytes();
        match self {
            Self::Iso => crate::iso8601::collected(text).map(|_| text.to_owned()),
            Self::DdMonYyyy if bytes.len() == 9 => day(
                number(&bytes[5..9])?,
                month(&bytes[2..5])?,
                number(&bytes[..2])?,
            ),
            Self::MdySlash | Self::DmySlash
                if bytes.len() == 10 && bytes[2] == b'/' && bytes[5] == b'/' =>
            {
                let (first, second) = (number(&bytes[..2])?, number(&bytes[3..5])?);
                let (month, date) = if self == Self::MdySlash {
                    (first, second)
                } else {
                    (second, first)
                };
                day(number(&bytes[6..])?, month, date)
            }
            Self::Yyyymmdd if bytes.len() == 8 => day(
                number(&bytes[..4])?,
                number(&bytes[4..6])?,
                number(&bytes[6..])?,
            ),
            Self::MonYyyy if bytes.len() == 7 => {
                let (year, month) = (number(&bytes[3..])?, month(&bytes[..3])?);
                ((1..=9999).contains(&year) && (1..=12).contains(&month))
                    .then(|| format!("{year:04}-{month:02}"))
            }
            _ => None,
        }
    }
}

/// The value of a field written as ASCII digits only.
fn number(digits: &[u8]) -> Option<u16> {
    digits.iter().try_fold(0u16, |value, byte| {
        byte.is_ascii_digit()
            .then(|| value * 10 + u16::from(byte - b'0'))
    })
}

/// The month a three-letter abbreviation names, in any letter case.
fn month(letters: &[u8]) -> Option<u16> {
    MONTHS
        .iter()
        .position(|name| letters.eq_ignore_ascii_case(*name))
        .map(|index| index as u16 + 1)
}

/// `YYYY-MM-DD` for a day of the proleptic Gregorian calendar.
fn day(year: u16, month: u16, day: u16) -> Option<String> {
    let month = u8::try_from(month).ok()?;
    let day = u8::try_from(day).ok()?;
    Date::new(year, month, day, Precision::Day)
        .ok()
        .map(|date| date.to_string())
}

/// A declared `forms` list that TIME-22 admits: at least one form, each
/// named once, and at most one of the two slash orders, in written order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateForms {
    /// The declared forms in written order, then `None`.
    forms: [Option<DateForm>; DateForm::ALL.len()],
}

impl DateForms {
    /// The admitted list `forms` writes, or each fault that refuses it.
    pub fn new(forms: &[DateForm]) -> Result<Self, Vec<FormsFault>> {
        let faults = faults(forms);
        if !faults.is_empty() {
            return Err(faults);
        }
        let mut held = [None; DateForm::ALL.len()];
        for (slot, form) in held.iter_mut().zip(forms) {
            *slot = Some(*form);
        }
        Ok(Self { forms: held })
    }

    /// The declared forms, in written order.
    pub fn iter(&self) -> impl Iterator<Item = DateForm> + '_ {
        self.forms.iter().map_while(|form| *form)
    }

    /// The collected date text that the first declared form reading `text`
    /// gives, or `None` when no declared form reads it.
    pub fn read(&self, text: &str) -> Option<String> {
        self.iter().find_map(|form| form.read(text))
    }
}

/// Why a written `forms` list is refused (TIME-22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormsFault {
    /// The list names no form.
    Empty,
    /// The entry at `index` names a form an earlier entry names.
    Repeated { index: usize, form: DateForm },
    /// The list names both `mdy_slash` and `dmy_slash`.
    BothSlashOrders,
}

impl FormsFault {
    /// Where the refusal is reported under the list's own path `forms`: a
    /// repeated form at its entry, any other fault at the list.
    pub fn path(self, forms: &str) -> String {
        match self {
            Self::Repeated { index, .. } => format!("{forms}[{index}]"),
            Self::Empty | Self::BothSlashOrders => forms.to_owned(),
        }
    }

    /// The facts the refusal reports: the operation, a repeated form as
    /// `value`, and what the list must be as `expected`.
    pub fn context(self) -> Vec<(&'static str, String)> {
        let mut context = vec![("operation", Verb::DateText.name().to_owned())];
        if let Self::Repeated { form, .. } = self {
            context.push(("value", form.name().to_owned()));
        }
        let expected = match self {
            Self::Empty => "at least one date form",
            Self::Repeated { .. } => "each date form once",
            Self::BothSlashOrders => "mdy_slash or dmy_slash, not both",
        };
        context.push(("expected", expected.to_owned()));
        context
    }

    /// The refusal at the `forms` of the `date_text` at `path`.
    pub fn diagnostic(self, path: &str) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::DateFormsList,
            spec_paths: vec![self.path(&format!("{path}.forms"))],
            context: self
                .context()
                .into_iter()
                .map(|(key, value)| (key.into(), Value::String(value)))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
}

/// Each fault of a written `forms` list, the list's own before its
/// entries': an empty list or both slash orders, then each entry that names
/// a form again, in written order.
pub fn faults(forms: &[DateForm]) -> Vec<FormsFault> {
    let mut faults = Vec::new();
    if forms.is_empty() {
        faults.push(FormsFault::Empty);
    }
    if forms.contains(&DateForm::MdySlash) && forms.contains(&DateForm::DmySlash) {
        faults.push(FormsFault::BothSlashOrders);
    }
    for (index, form) in forms.iter().enumerate() {
        if forms[..index].contains(form) {
            faults.push(FormsFault::Repeated { index, form: *form });
        }
    }
    faults
}

#[cfg(test)]
mod tests {
    //! The reader's own seam, which no run reaches one form at a time: each
    //! form's reading, and a written list's faults before any document is
    //! lowered. The runs that read declared lists are the adapters fixture
    //! `date_text_forms.json` and the benchmarks TIME-22 cites.
    use super::*;

    /// Each form reads the whole text in its shape into the collected date
    /// text it denotes, and nothing else (TIME-22): `iso` keeps collected
    /// date text as written, its unknown fields and its time included;
    /// month abbreviations take any letter case; days and months are two
    /// digits and years four; nothing is trimmed; and a field outside its
    /// VALUE-15 range is no date.
    #[test]
    fn each_form_reads_its_own_shape_into_collected_date_text() {
        use DateForm as F;
        for (form, text, expected) in [
            (F::Iso, "2001-03-04", Some("2001-03-04")),
            (F::Iso, "2001-03", Some("2001-03")),
            (F::Iso, "2001", Some("2001")),
            (F::Iso, "2001---04", Some("2001---04")),
            (F::Iso, "2001-03-04T08:30", Some("2001-03-04T08:30")),
            (F::Iso, "2001-02-29", None),
            (F::Iso, "2001-03-04/2001-03-10", None),
            (F::Iso, "20010304", None),
            (F::DdMonYyyy, "04MAR2001", Some("2001-03-04")),
            (F::DdMonYyyy, "04mar2001", Some("2001-03-04")),
            (F::DdMonYyyy, "04Dec2001", Some("2001-12-04")),
            (F::DdMonYyyy, "29FEB2000", Some("2000-02-29")),
            (F::DdMonYyyy, "29FEB2001", None),
            (F::DdMonYyyy, "4MAR2001", None),
            (F::DdMonYyyy, "04MAR01", None),
            (F::DdMonYyyy, "04MRZ2001", None),
            (F::DdMonYyyy, "04MAR0000", None),
            (F::DdMonYyyy, " 04MAR2001", None),
            (F::DdMonYyyy, "04MAR2001 ", None),
            (F::MdySlash, "03/04/2001", Some("2001-03-04")),
            (F::MdySlash, "12/31/2001", Some("2001-12-31")),
            (F::MdySlash, "13/04/2001", None),
            (F::MdySlash, "3/4/2001", None),
            (F::MdySlash, "03/04/01", None),
            (F::MdySlash, "03-04-2001", None),
            (F::DmySlash, "03/04/2001", Some("2001-04-03")),
            (F::DmySlash, "31/12/2001", Some("2001-12-31")),
            (F::DmySlash, "12/31/2001", None),
            (F::Yyyymmdd, "20010304", Some("2001-03-04")),
            (F::Yyyymmdd, "20010230", None),
            (F::Yyyymmdd, "200103", None),
            (F::Yyyymmdd, "2001-03-04", None),
            (F::MonYyyy, "MAR2001", Some("2001-03")),
            (F::MonYyyy, "mar2001", Some("2001-03")),
            (F::MonYyyy, "MAR01", None),
            (F::MonYyyy, "04MAR2001", None),
            (F::MonYyyy, "MARCH2001", None),
        ] {
            assert_eq!(
                form.read(text).as_deref(),
                expected,
                "{} reading {text:?}",
                form.name()
            );
        }
    }

    /// Only the two slash orders, which no list declares together, share a
    /// shape, so in an admitted list at most one form reads a text, whatever
    /// the order (TIME-22).
    #[test]
    fn only_the_two_slash_orders_share_a_shape() {
        for (text, expected) in [
            ("2001-03-04", &["iso"][..]),
            ("2001", &["iso"]),
            ("04MAR2001", &["ddmonyyyy"]),
            ("03/04/2001", &["mdy_slash", "dmy_slash"]),
            ("20010304", &["yyyymmdd"]),
            ("MAR2001", &["monyyyy"]),
            ("4 March 2001", &[]),
        ] {
            let readers = DateForm::ALL
                .into_iter()
                .filter(|form| form.read(text).is_some())
                .map(DateForm::name)
                .collect::<Vec<_>>();
            assert_eq!(readers, expected, "{text:?}");
        }
    }

    /// A list names at least one form, each once, and not both slash
    /// orders; the list's own faults come before its entries' (TIME-22).
    #[test]
    fn a_forms_list_is_refused_for_each_of_its_faults() {
        use DateForm as F;
        for (forms, expected, label) in [
            (vec![F::Iso, F::MdySlash], vec![], "one slash order"),
            (vec![], vec![FormsFault::Empty], "no form"),
            (
                vec![F::MdySlash, F::DmySlash],
                vec![FormsFault::BothSlashOrders],
                "both slash orders",
            ),
            (
                vec![F::Iso, F::MonYyyy, F::Iso, F::Iso],
                vec![
                    FormsFault::Repeated {
                        index: 2,
                        form: F::Iso,
                    },
                    FormsFault::Repeated {
                        index: 3,
                        form: F::Iso,
                    },
                ],
                "a form named three times",
            ),
            (
                vec![F::DmySlash, F::MdySlash, F::DmySlash],
                vec![
                    FormsFault::BothSlashOrders,
                    FormsFault::Repeated {
                        index: 2,
                        form: F::DmySlash,
                    },
                ],
                "both orders, one named twice",
            ),
        ] {
            assert_eq!(faults(&forms), expected, "{label}");
        }
    }

    /// An admitted list keeps its written order and reads with its forms
    /// only: without `monyyyy`, `MAR2001` is no date.
    #[test]
    fn an_admitted_list_reads_with_its_declared_forms_only() {
        use DateForm as F;
        let forms = DateForms::new(&[F::Yyyymmdd, F::DmySlash, F::Iso]).unwrap();
        assert_eq!(
            forms.iter().collect::<Vec<_>>(),
            [F::Yyyymmdd, F::DmySlash, F::Iso]
        );
        for (text, expected) in [
            ("20010304", Some("2001-03-04")),
            ("03/04/2001", Some("2001-04-03")),
            ("2001-03", Some("2001-03")),
            ("MAR2001", None),
        ] {
            assert_eq!(forms.read(text).as_deref(), expected, "{text:?}");
        }
    }
}
