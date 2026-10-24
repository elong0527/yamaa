//! The reader of the JSON the diagnostic fixtures are written in (#3691).
//!
//! Core depends on nothing else in the workspace and its tests import no
//! other crate's test source (#2158), so the adapters' decoder is out of
//! reach and this reads RFC 8259 JSON itself. It keeps what the fixtures
//! need: an object's members in written order, since a document's mappings
//! keep theirs, and a number as its written token, since a context value
//! compares by its exact spelling. A repeated member name, a malformed token
//! or text after the value fails the test at the byte it starts.
use yamaa_core::json::{Number, Value};

/// A JSON value as written.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// Read the JSON text of the fixture `name`.
#[track_caller]
pub fn read(text: &str, name: &str) -> Json {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
        name,
    };
    let value = reader.value();
    reader.space();
    if reader.at != reader.bytes.len() {
        reader.fail("text after the value");
    }
    value
}

impl Json {
    /// The member `key` of an object, if it has one.
    #[track_caller]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            other => panic!("{key:?} is read from an object, not {other:?}"),
        }
    }

    /// The member `key` of an object, which it must have.
    #[track_caller]
    pub fn field(&self, key: &str) -> &Json {
        self.get(key)
            .unwrap_or_else(|| panic!("no {key:?} member in {self:?}"))
    }

    /// This object, whose members must all be among `members`, so a
    /// misspelt or unknown member fails rather than being ignored.
    #[track_caller]
    pub fn closed(&self, members: &[&str]) -> &Json {
        for (name, _) in self.members() {
            assert!(
                members.contains(&name.as_str()),
                "unknown member {name:?}; the members are {members:?}"
            );
        }
        self
    }

    #[track_caller]
    pub fn text(&self) -> &str {
        match self {
            Self::String(text) => text,
            other => panic!("expected a string, not {other:?}"),
        }
    }

    #[track_caller]
    pub fn boolean(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            other => panic!("expected a Boolean, not {other:?}"),
        }
    }

    /// An integer member, parsed as the type the reader asks for.
    #[track_caller]
    pub fn integer<T: std::str::FromStr>(&self) -> T {
        match self {
            Self::Number(token) => token
                .parse()
                .unwrap_or_else(|_| panic!("{token} is not an integer of the type read")),
            other => panic!("expected an integer, not {other:?}"),
        }
    }

    #[track_caller]
    pub fn items(&self) -> &[Json] {
        match self {
            Self::Array(items) => items,
            other => panic!("expected an array, not {other:?}"),
        }
    }

    #[track_caller]
    pub fn members(&self) -> &[(String, Json)] {
        match self {
            Self::Object(members) => members,
            other => panic!("expected an object, not {other:?}"),
        }
    }

    /// This value as a diagnostic context value, whose objects order their
    /// keys and whose numbers keep their token.
    pub fn value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Number(token) => Value::Number(
                token
                    .parse::<Number>()
                    .expect("the reader admits only JSON number tokens"),
            ),
            Self::String(text) => Value::String(text.clone()),
            Self::Array(items) => Value::Array(items.iter().map(Self::value).collect()),
            Self::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|(name, value)| (name.clone(), value.value()))
                    .collect(),
            ),
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    name: &'a str,
}

impl Reader<'_> {
    #[track_caller]
    fn fail(&self, reason: &str) -> ! {
        panic!("{}: {reason} at byte {}", self.name, self.at)
    }

    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    #[track_caller]
    fn expect(&mut self, byte: u8) {
        self.space();
        if self.bytes.get(self.at) != Some(&byte) {
            self.fail(&format!("expected {:?}", byte as char));
        }
        self.at += 1;
    }

    #[track_caller]
    fn value(&mut self) -> Json {
        self.space();
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Json::String(self.string()),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => {
                for (word, value) in [
                    ("null", Json::Null),
                    ("true", Json::Bool(true)),
                    ("false", Json::Bool(false)),
                ] {
                    if self.bytes[self.at..].starts_with(word.as_bytes()) {
                        self.at += word.len();
                        return value;
                    }
                }
                self.fail("expected a value")
            }
        }
    }

    #[track_caller]
    fn object(&mut self) -> Json {
        self.expect(b'{');
        let mut members: Vec<(String, Json)> = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Json::Object(members);
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                self.fail("expected a member name");
            }
            let name = self.string();
            if members.iter().any(|(written, _)| *written == name) {
                self.fail(&format!("the member {name:?} is repeated"));
            }
            self.expect(b':');
            let value = self.value();
            members.push((name, value));
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Json::Object(members);
                }
                _ => self.fail("expected ',' or '}'"),
            }
        }
    }

    #[track_caller]
    fn array(&mut self) -> Json {
        self.expect(b'[');
        let mut items = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Json::Array(items);
        }
        loop {
            items.push(self.value());
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Json::Array(items);
                }
                _ => self.fail("expected ',' or ']'"),
            }
        }
    }

    /// A number token, which `Number` admits exactly as JSON spells it.
    #[track_caller]
    fn number(&mut self) -> Json {
        let start = self.at;
        while matches!(
            self.bytes.get(self.at),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.at += 1;
        }
        let token = std::str::from_utf8(&self.bytes[start..self.at]).expect("ASCII token");
        if token.parse::<Number>().is_err() {
            self.at = start;
            self.fail(&format!("{token} is not a JSON number"));
        }
        Json::Number(token.into())
    }

    #[track_caller]
    fn string(&mut self) -> String {
        self.expect(b'"');
        let mut text = String::new();
        loop {
            let start = self.at;
            while !matches!(self.bytes.get(self.at), Some(b'"' | b'\\') | None) {
                if self.bytes[self.at] < 0x20 {
                    self.fail("a control character must be escaped");
                }
                self.at += 1;
            }
            text.push_str(
                std::str::from_utf8(&self.bytes[start..self.at])
                    .unwrap_or_else(|_| self.fail("invalid UTF-8")),
            );
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return text;
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.bytes.get(self.at).copied();
                    self.at += 1;
                    match escape {
                        Some(b'"') => text.push('"'),
                        Some(b'\\') => text.push('\\'),
                        Some(b'/') => text.push('/'),
                        Some(b'b') => text.push('\u{8}'),
                        Some(b'f') => text.push('\u{c}'),
                        Some(b'n') => text.push('\n'),
                        Some(b'r') => text.push('\r'),
                        Some(b't') => text.push('\t'),
                        Some(b'u') => {
                            let unit = self.unit();
                            let scalar = if (0xD800..0xDC00).contains(&unit) {
                                if !self.bytes[self.at..].starts_with(b"\\u") {
                                    self.fail("a high surrogate needs its low surrogate");
                                }
                                self.at += 2;
                                let low = self.unit();
                                if !(0xDC00..0xE000).contains(&low) {
                                    self.fail("a high surrogate needs its low surrogate");
                                }
                                0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                unit
                            };
                            text.push(
                                char::from_u32(scalar)
                                    .unwrap_or_else(|| self.fail("a lone surrogate")),
                            );
                        }
                        _ => self.fail("an unknown escape"),
                    }
                }
                _ => self.fail("an unterminated string"),
            }
        }
    }

    #[track_caller]
    fn unit(&mut self) -> u32 {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .filter(|digits| digits.iter().all(u8::is_ascii_hexdigit))
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .unwrap_or_else(|| self.fail("expected four hex digits"));
        self.at += 4;
        digits
    }
}
