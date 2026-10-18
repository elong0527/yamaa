//! ISO 8601 text forms: the date and time submission data types a `str`
//! column may declare (SUB-26), collected date text with unknown components
//! (TIME-7), and the interval and duration text that is not date text
//! (TIME-13).

use crate::temporal::month_length;

/// One date or time component: its separator before it, its width, and its
/// range. The year has no separator, and the hour is written after `T`.
struct Component {
    separator: u8,
    width: usize,
    low: u16,
    high: u16,
}

const COMPONENTS: [Component; 6] = [
    Component {
        separator: b'-',
        width: 4,
        low: 1,
        high: 9999,
    },
    Component {
        separator: b'-',
        width: 2,
        low: 1,
        high: 12,
    },
    Component {
        separator: b'-',
        width: 2,
        low: 1,
        high: 31,
    },
    Component {
        separator: b'T',
        width: 2,
        low: 0,
        high: 23,
    },
    Component {
        separator: b':',
        width: 2,
        low: 0,
        high: 59,
    },
    Component {
        separator: b':',
        width: 2,
        low: 0,
        high: 59,
    },
];
/// The index of the hour in [`COMPONENTS`].
const HOUR: usize = 3;
/// The index of the second in [`COMPONENTS`].
const SECOND: usize = 5;

/// A read component: its digits' value, or `None` when written `-`.
pub(crate) type Field = Option<u16>;

/// A cursor over ASCII text; any other byte fails every form.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            at: 0,
        }
    }
    fn done(&self) -> bool {
        self.at == self.bytes.len()
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }
    /// Exactly `width` ASCII digits.
    fn digits(&mut self, width: usize) -> Option<u16> {
        let slice = self.bytes.get(self.at..self.at + width)?;
        let value = slice.iter().try_fold(0u16, |number, byte| {
            byte.is_ascii_digit()
                .then(|| number * 10 + u16::from(byte - b'0'))
        })?;
        self.at += width;
        Some(value)
    }
    /// One or more ASCII digits.
    fn some_digits(&mut self) -> bool {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        self.at > start
    }
}

/// Read components `first` through at most `last` in order, each its digits
/// within its range or, where `unknown` is admitted, `-`. The first is read
/// without its separator; reading stops before a byte that is not the next
/// separator, and a separator must be followed by its component. A day lies
/// within the month it names, or within 31 when its month is unknown and 29
/// in February of an unknown year.
fn components(
    cursor: &mut Cursor<'_>,
    first: usize,
    last: usize,
    unknown: bool,
) -> Option<Vec<Field>> {
    let mut fields = Vec::new();
    for (index, component) in COMPONENTS.iter().enumerate().take(last + 1).skip(first) {
        if index > first && !cursor.eat(component.separator) {
            break;
        }
        let field = if unknown && cursor.peek() == Some(b'-') {
            cursor.at += 1;
            None
        } else {
            let value = cursor.digits(component.width)?;
            if !(component.low..=component.high).contains(&value) {
                return None;
            }
            Some(value)
        };
        fields.push(field);
    }
    if fields.is_empty() {
        return None;
    }
    // Day within its month, when the day is known and lies in the read range.
    if first == 0 && fields.len() > 2 {
        if let Some(day) = fields[2] {
            let longest = match (fields[0], fields[1]) {
                (Some(year), Some(month)) => month_length(year, month as u8)?,
                (None, Some(2)) => 29,
                (None, Some(month)) => month_length(2000, month as u8)?,
                (_, None) => 31,
            };
            if day > u16::from(longest) {
                return None;
            }
        }
    }
    Some(fields)
}

/// An optional fraction `.` digits after a known second.
fn fraction(cursor: &mut Cursor<'_>, second: bool) -> bool {
    if cursor.peek() != Some(b'.') {
        return true;
    }
    second && {
        cursor.at += 1;
        cursor.some_digits()
    }
}

/// An optional zone designator, `Z` or a signed `hh:mm` offset.
fn zone(cursor: &mut Cursor<'_>) -> bool {
    match cursor.peek() {
        Some(b'Z') => {
            cursor.at += 1;
            true
        }
        Some(b'+' | b'-') => {
            cursor.at += 1;
            cursor.digits(2).is_some_and(|hour| hour <= 23)
                && cursor.eat(b':')
                && cursor.digits(2).is_some_and(|minute| minute <= 59)
        }
        _ => true,
    }
}

/// A date or time component sequence followed by an optional fraction and
/// zone. `exact` requires every component from `first` to `last`.
fn moment(text: &str, first: usize, last: usize, unknown: bool, exact: bool) -> bool {
    let mut cursor = Cursor::new(text);
    let Some(fields) = components(&mut cursor, first, last, unknown) else {
        return false;
    };
    let read = first + fields.len() - 1;
    if exact && read != last {
        return false;
    }
    if unknown && fields.iter().all(Option::is_none) {
        return false;
    }
    let second = read == SECOND && fields.last().copied().flatten().is_some();
    let timed = read >= HOUR && last >= HOUR;
    fraction(&mut cursor, second) && (!timed || zone(&mut cursor)) && cursor.done()
}

/// ISO 8601 fixed-length or calendar duration text, `PnW` or
/// `P[nY][nM][nD][T[nH][nM][nS]]` with an optional leading `-`; at least one
/// component, `T` before a time component, and a decimal fraction, `.` or
/// `,`, on the last component only.
pub fn duration(text: &str) -> bool {
    let mut cursor = Cursor::new(text);
    cursor.eat(b'-');
    if !cursor.eat(b'P') {
        return false;
    }
    let mut count = 0;
    let mut fractional = false;
    let mut component = |cursor: &mut Cursor<'_>, designators: &[u8]| -> Option<usize> {
        let mut seen = 0;
        let mut allowed = designators;
        while cursor.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            if fractional {
                return None;
            }
            cursor.some_digits();
            if matches!(cursor.peek(), Some(b'.' | b',')) {
                cursor.at += 1;
                if !cursor.some_digits() {
                    return None;
                }
                fractional = true;
            }
            let designator = cursor.peek()?;
            let position = allowed.iter().position(|&byte| byte == designator)?;
            cursor.at += 1;
            allowed = &allowed[position + 1..];
            seen += 1;
        }
        Some(seen)
    };
    let Some(date) = component(&mut cursor, b"YMWD") else {
        return false;
    };
    count += date;
    if cursor.eat(b'T') {
        let Some(time) = component(&mut cursor, b"HMS") else {
            return false;
        };
        if time == 0 {
            return false;
        }
        count += time;
    }
    // A week stands alone.
    let week = text.contains('W');
    count > 0 && cursor.done() && (!week || count == 1)
}

/// ISO 8601 interval text: two parts joined by one `/`, each partial date
/// and time text or a duration, not both durations.
pub fn interval(text: &str) -> bool {
    let mut parts = text.split('/');
    let (Some(start), Some(end), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let point = |part: &str| Form::PartialDatetime.admits(part);
    (point(start) || duration(start))
        && (point(end) || duration(end))
        && !(duration(start) && duration(end))
}

/// Whether text is an ISO 8601 interval or duration rather than date text.
pub fn interval_or_duration(text: &str) -> bool {
    duration(text) || interval(text)
}

/// Collected date text (TIME-7): components from the year, each its digits
/// or `-` when unknown, at least one known, with neither a fraction nor a
/// zone. Returns every component read, or `None` when the text is not
/// collected date text.
pub(crate) fn collected(text: &str) -> Option<Vec<Field>> {
    let mut cursor = Cursor::new(text);
    let fields = components(&mut cursor, 0, SECOND, true)?;
    (cursor.done() && fields.iter().any(Option::is_some)).then_some(fields)
}

/// The ISO 8601 form of a date or time submission data type (SUB-26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Date,
    Time,
    Datetime,
    PartialDate,
    PartialTime,
    PartialDatetime,
    IncompleteDate,
    IncompleteTime,
    IncompleteDatetime,
    DurationDatetime,
    IntervalDatetime,
}

impl Form {
    /// Every form, in the order SUB-26 lists them.
    pub const ALL: [Form; 11] = [
        Self::Date,
        Self::Time,
        Self::Datetime,
        Self::PartialDate,
        Self::PartialTime,
        Self::PartialDatetime,
        Self::IncompleteDate,
        Self::IncompleteTime,
        Self::IncompleteDatetime,
        Self::DurationDatetime,
        Self::IntervalDatetime,
    ];
    /// The submission data type that names the form.
    pub fn name(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Time => "time",
            Self::Datetime => "datetime",
            Self::PartialDate => "partialDate",
            Self::PartialTime => "partialTime",
            Self::PartialDatetime => "partialDatetime",
            Self::IncompleteDate => "incompleteDate",
            Self::IncompleteTime => "incompleteTime",
            Self::IncompleteDatetime => "incompleteDatetime",
            Self::DurationDatetime => "durationDatetime",
            Self::IntervalDatetime => "intervalDatetime",
        }
    }
    /// The form a submission data type names, or `None` for a type that is
    /// not a date or time type.
    pub fn of(data_type: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|form| form.name() == data_type)
    }
    /// Whether `text` has the form.
    pub fn admits(self, text: &str) -> bool {
        match self {
            Self::Date => moment(text, 0, 2, false, true),
            Self::Time => moment(text, HOUR, SECOND, false, true),
            Self::Datetime => moment(text, 0, SECOND, false, true),
            Self::PartialDate => moment(text, 0, 2, false, false),
            Self::PartialTime => moment(text, HOUR, SECOND, false, false),
            Self::PartialDatetime => moment(text, 0, SECOND, false, false),
            Self::IncompleteDate => moment(text, 0, 2, true, false),
            Self::IncompleteTime => moment(text, HOUR, SECOND, true, false),
            Self::IncompleteDatetime => moment(text, 0, SECOND, true, false),
            Self::DurationDatetime => duration(text),
            Self::IntervalDatetime => interval(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collected_text_keeps_every_component_and_its_unknown_ones() {
        assert_eq!(collected("2025"), Some(vec![Some(2025)]));
        assert_eq!(
            collected("2025---15"),
            Some(vec![Some(2025), None, Some(15)])
        );
        assert_eq!(collected("--12-15"), Some(vec![None, Some(12), Some(15)]));
        assert_eq!(
            collected("-----T07:15"),
            Some(vec![None, None, None, Some(7), Some(15)])
        );
        for refused in [
            "",
            "-",
            "---",
            "-----T-",
            "2025-",
            "2025---",
            "2025-02-30",
            "--02-30",
        ] {
            assert_eq!(collected(refused), None, "{refused}");
        }
    }

    #[test]
    fn durations_and_intervals_are_recognised_and_dates_are_not() {
        for text in [
            "P3D",
            "PT2H30M",
            "P1Y2M10DT2H30M",
            "P2W",
            "-PT15M",
            "PT0.5S",
            "P1,5D",
        ] {
            assert!(duration(text), "{text}");
            assert!(interval_or_duration(text), "{text}");
        }
        for text in ["P", "PT", "P1H", "P2W1D", "P1.5DT1H", "P1M1Y", "3D", "P3d"] {
            assert!(!duration(text), "{text}");
        }
        for text in [
            "2025-01-01/2025-01-10",
            "2025-01-01T08:00/P1D",
            "P1D/2025-01-10",
            "2025/2026",
        ] {
            assert!(interval(text), "{text}");
        }
        for text in [
            "P1D/P2D",
            "2025/03/04",
            "2025-01-01/",
            "/2025",
            "2025-01-01",
        ] {
            assert!(!interval(text), "{text}");
        }
    }
}
