//! PEP 440 versions and the version specifiers that marker comparisons build,
//! as Python's `packaging` 26 parses, orders, prints and matches them.
use std::cmp::Ordering;
use std::fmt;

/// A non-negative integer of any length, held as ASCII digits without leading
/// zeros, so that numbers compare by value and none overflows.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Number(String);

impl Number {
    fn new(digits: &str) -> Self {
        let digits = digits.trim_start_matches('0');
        Self(if digits.is_empty() { "0" } else { digits }.to_owned())
    }

    fn zero() -> Self {
        Self("0".to_owned())
    }

    fn is_zero(&self) -> bool {
        self.0 == "0"
    }

    /// The next integer.
    fn successor(&self) -> Self {
        let mut digits = self.0.clone().into_bytes();
        let mut index = digits.len();
        loop {
            if index == 0 {
                digits.insert(0, b'1');
                break;
            }
            index -= 1;
            if digits[index] == b'9' {
                digits[index] = b'0';
            } else {
                digits[index] += 1;
                break;
            }
        }
        Self(String::from_utf8(digits).expect("ASCII digits"))
    }
}

impl Ord for Number {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A pre-release phase, in its order: alpha, beta, release candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Alpha,
    Beta,
    Candidate,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Alpha => "a",
            Self::Beta => "b",
            Self::Candidate => "rc",
        }
    }
}

/// One part of a local version label. Text sorts before a number, text
/// compares lexically and numbers by value.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Local {
    Text(String),
    Number(Number),
}

impl Ord for Local {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => left.cmp(right),
            (Self::Number(left), Self::Number(right)) => left.cmp(right),
            (Self::Text(_), Self::Number(_)) => Ordering::Less,
            (Self::Number(_), Self::Text(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Local {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A parsed PEP 440 version. Two versions are equal when they denote the same
/// version, as `1.0` and `1.0.0` do; [`fmt::Display`] prints the normalized
/// form of the text it was parsed from, trailing zeros kept.
#[derive(Clone, Debug)]
pub(crate) struct Version {
    epoch: Number,
    release: Vec<Number>,
    pre: Option<(Phase, Number)>,
    post: Option<Number>,
    dev: Option<Number>,
    local: Option<Vec<Local>>,
}

/// The release without its trailing zeros, the part that ordering compares.
fn significant(release: &[Number]) -> &[Number] {
    let end = release
        .iter()
        .rposition(|number| !number.is_zero())
        .map_or(0, |index| index + 1);
    &release[..end]
}

impl Version {
    fn release(epoch: Number, release: Vec<Number>) -> Self {
        Self {
            epoch,
            release,
            pre: None,
            post: None,
            dev: None,
            local: None,
        }
    }

    /// The pre-release, post-release and development key: a development
    /// release with neither of the others sorts before every pre-release, a
    /// final release after them, a post-release after its final release and
    /// a development release before the release it develops. An absent
    /// number compares only with another absent one.
    #[allow(clippy::type_complexity)]
    fn suffix(
        &self,
    ) -> (
        i8,
        Option<&Number>,
        u8,
        Option<&Number>,
        u8,
        Option<&Number>,
    ) {
        let (phase, number) = match (&self.pre, &self.post, &self.dev) {
            (Some((phase, number)), _, _) => (*phase as i8, Some(number)),
            (None, None, Some(_)) => (-1, None),
            (None, _, _) => (3, None),
        };
        (
            phase,
            number,
            u8::from(self.post.is_some()),
            self.post.as_ref(),
            u8::from(self.dev.is_none()),
            self.dev.as_ref(),
        )
    }

    /// The significant digits of the longest number that this version's
    /// normalized text prints: the epoch only when it is not zero.
    fn printed_digits(&self) -> usize {
        let epoch = (!self.epoch.is_zero()).then_some(&self.epoch);
        let local = self.local.iter().flatten().filter_map(|part| match part {
            Local::Number(number) => Some(number),
            Local::Text(_) => None,
        });
        self.release
            .iter()
            .chain(epoch)
            .chain(self.pre.as_ref().map(|(_, number)| number))
            .chain(self.post.as_ref())
            .chain(self.dev.as_ref())
            .chain(local)
            .map(|number| number.0.len())
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn is_prerelease(&self) -> bool {
        self.pre.is_some() || self.dev.is_some()
    }

    /// Whether `other` is this version or one of its local variants: the same
    /// public version, with any local label.
    fn same_public(&self, other: &Self) -> bool {
        self.same_base(other)
            && self.pre == other.pre
            && self.post == other.post
            && self.dev == other.dev
    }

    /// Whether `other` has this version's epoch and release.
    fn same_base(&self, other: &Self) -> bool {
        self.epoch == other.epoch && significant(&self.release) == significant(&other.release)
    }

    /// This version with only the given development release after its
    /// pre-release and post-release.
    fn with_development(&self, dev: Number) -> Self {
        Self {
            dev: Some(dev),
            local: None,
            ..self.clone()
        }
    }

    /// The first development release of the next release at this version's
    /// last release number: `1.2` gives `1.3.dev0`.
    fn next_prefix(epoch: &Number, release: &[Number]) -> Self {
        let mut release = release.to_vec();
        if let Some(last) = release.last_mut() {
            *last = last.successor();
        }
        Self::release(epoch.clone(), release).with_development(Number::zero())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then_with(|| significant(&self.release).cmp(significant(&other.release)))
            .then_with(|| self.suffix().cmp(&other.suffix()))
            .then_with(|| self.local.cmp(&other.local))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.epoch.is_zero() {
            write!(f, "{}!", self.epoch.0)?;
        }
        for (index, number) in self.release.iter().enumerate() {
            if index > 0 {
                f.write_str(".")?;
            }
            f.write_str(&number.0)?;
        }
        if let Some((phase, number)) = &self.pre {
            write!(f, "{}{}", phase.label(), number.0)?;
        }
        if let Some(number) = &self.post {
            write!(f, ".post{}", number.0)?;
        }
        if let Some(number) = &self.dev {
            write!(f, ".dev{}", number.0)?;
        }
        if let Some(parts) = &self.local {
            for (index, part) in parts.iter().enumerate() {
                f.write_str(if index == 0 { "+" } else { "." })?;
                match part {
                    Local::Text(text) => f.write_str(text)?,
                    Local::Number(number) => f.write_str(&number.0)?,
                }
            }
        }
        Ok(())
    }
}

/// Whether Python's `str.isspace` holds for `c`, which is what `\s` matches
/// and `str.strip` removes: Unicode white space and the four information
/// separators.
pub(crate) fn is_python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// CPython's default limit on the digits of an integer converted from text
/// (`sys.int_info.default_max_str_digits`).
pub(crate) const DIGITS: usize = 4300;

/// The digits up to which CPython converts text to an integer whatever its
/// limit (`sys.int_info.str_digits_check_threshold`); a limit, when on, is no
/// smaller.
pub(crate) const THRESHOLD: usize = 640;

/// CPython's limit on the digits of an integer converted from text, which
/// `packaging` converted every number of a version under, with `int()`:
/// leading zeros count. Python lets the limit be raised, lowered or turned
/// off, so the engine reads the interpreter's own, and only when a number of
/// more than [`THRESHOLD`] digits needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    /// The interpreter's limit is not read yet.
    Unread,
    /// The interpreter's limit: `None` when it is turned off.
    Read(Option<usize>),
}

impl Limit {
    /// Refuse a version whose longest number has `digits` digits when it does
    /// not convert, or when that needs the limit, which is unread.
    fn check(self, digits: usize) -> Result<(), Refusal> {
        match self {
            _ if digits <= THRESHOLD => Ok(()),
            Self::Unread => Err(Refusal::Unread),
            Self::Read(Some(limit)) if digits > limit => Err(Refusal::Digits { digits, limit }),
            Self::Read(_) => Ok(()),
        }
    }
}

/// Why a text is not read as a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The text is no PEP 440 version.
    Invalid,
    /// The text is a PEP 440 version, but its longest number has `digits`
    /// digits, more than the interpreter's `limit`.
    Digits { digits: usize, limit: usize },
    /// The text is a PEP 440 version with a number of more than
    /// [`THRESHOLD`] digits, so it needs the interpreter's limit, which is
    /// unread.
    Unread,
}

/// What a version text may hold besides the release: everything, as in a
/// version; or what each specifier operator allows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grammar {
    Version,
    /// `==` and `!=`: a version with a local label, or a release with `.*`.
    Equality,
    /// `~=`: no local label, and at least two release numbers.
    Compatible,
    /// `<`, `<=`, `>` and `>=`: no local label.
    Ordered,
}

struct Cursor<'a> {
    text: &'a str,
    at: usize,
    /// The digits of the longest number read.
    longest: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn eat(&mut self, c: char) -> bool {
        if self.rest().starts_with(c) {
            self.at += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn separator(&mut self) {
        let _ = self.eat('.') || self.eat('_') || self.eat('-');
    }

    fn digits(&mut self) -> Option<&'a str> {
        let length = self.rest().bytes().take_while(u8::is_ascii_digit).count();
        (length > 0).then(|| {
            let digits = &self.rest()[..length];
            self.at += length;
            digits
        })
    }

    fn alphanumerics(&mut self) -> Option<&'a str> {
        let length = self
            .rest()
            .bytes()
            .take_while(u8::is_ascii_alphanumeric)
            .count();
        (length > 0).then(|| {
            let part = &self.rest()[..length];
            self.at += length;
            part
        })
    }

    /// The first of `words` that the text continues with, ignoring ASCII case.
    fn word<T: Copy>(&mut self, words: &[(&str, T)]) -> Option<T> {
        let rest = self.rest().as_bytes();
        let (word, value) = words.iter().find(|(word, _)| {
            rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word.as_bytes())
        })?;
        self.at += word.len();
        Some(*value)
    }

    /// Read `digits` as a number, noting how long the longest is.
    fn read(&mut self, digits: &str) -> Number {
        self.longest = self.longest.max(digits.len());
        Number::new(digits)
    }

    fn number(&mut self) -> Number {
        match self.digits() {
            Some(digits) => self.read(digits),
            None => Number::zero(),
        }
    }
}

/// A version text that a grammar admits.
struct Parsed {
    version: Version,
    /// Whether the text ended in `.*`.
    wildcard: bool,
    /// The digits of its longest number, which `packaging` converted only
    /// when the interpreter's limit allowed.
    longest: usize,
}

/// Parse `text` as a version that `grammar` admits, after Python white space
/// at either end. Each optional part is taken whole or not at all, as
/// `packaging`'s possessive pattern takes it. The grammar decides first: a
/// number of any length is admitted, and the longest is noted for the
/// interpreter's limit.
fn parse(text: &str, grammar: Grammar) -> Option<Parsed> {
    let text = text.trim_matches(is_python_space);
    let mut cursor = Cursor {
        text,
        at: 0,
        longest: 0,
    };
    cursor.word(&[("v", ())]);
    let mut epoch = Number::zero();
    let start = cursor.at;
    if let Some(digits) = cursor.digits() {
        if cursor.eat('!') {
            epoch = cursor.read(digits);
        } else {
            cursor.at = start;
        }
    }
    let first = cursor.digits()?;
    let mut release = vec![cursor.read(first)];
    loop {
        let at = cursor.at;
        if cursor.eat('.') {
            if let Some(digits) = cursor.digits() {
                release.push(cursor.read(digits));
                continue;
            }
        }
        cursor.at = at;
        break;
    }
    let mut version = Version::release(epoch, release);
    if grammar == Grammar::Equality && cursor.rest() == ".*" {
        return Some(Parsed {
            version,
            wildcard: true,
            longest: cursor.longest,
        });
    }
    if grammar == Grammar::Compatible && version.release.len() < 2 {
        return None;
    }
    let at = cursor.at;
    cursor.separator();
    let phase = cursor.word(&[
        ("alpha", Phase::Alpha),
        ("a", Phase::Alpha),
        ("beta", Phase::Beta),
        ("b", Phase::Beta),
        ("preview", Phase::Candidate),
        ("pre", Phase::Candidate),
        ("c", Phase::Candidate),
        ("rc", Phase::Candidate),
    ]);
    match phase {
        Some(phase) => {
            cursor.separator();
            version.pre = Some((phase, cursor.number()));
        }
        None => cursor.at = at,
    }
    let at = cursor.at;
    if cursor.eat('-') {
        if let Some(digits) = cursor.digits() {
            version.post = Some(cursor.read(digits));
        }
    }
    if version.post.is_none() {
        cursor.at = at;
        cursor.separator();
        if cursor
            .word(&[("post", ()), ("rev", ()), ("r", ())])
            .is_some()
        {
            cursor.separator();
            version.post = Some(cursor.number());
        } else {
            cursor.at = at;
        }
    }
    let at = cursor.at;
    cursor.separator();
    if cursor.word(&[("dev", ())]).is_some() {
        cursor.separator();
        version.dev = Some(cursor.number());
    } else {
        cursor.at = at;
    }
    if matches!(grammar, Grammar::Version | Grammar::Equality) {
        let at = cursor.at;
        if cursor.eat('+') {
            let mut parts = Vec::new();
            while let Some(part) = cursor.alphanumerics() {
                parts.push(if part.bytes().all(|byte| byte.is_ascii_digit()) {
                    Local::Number(cursor.read(part))
                } else {
                    Local::Text(part.to_ascii_lowercase())
                });
                let next = cursor.at;
                cursor.separator();
                // A separator continues the label only before another part.
                if !cursor
                    .rest()
                    .starts_with(|c: char| c.is_ascii_alphanumeric())
                {
                    cursor.at = next;
                    break;
                }
            }
            if parts.is_empty() {
                cursor.at = at;
            } else {
                version.local = Some(parts);
            }
        }
    }
    cursor.rest().is_empty().then_some(Parsed {
        version,
        wildcard: false,
        longest: cursor.longest,
    })
}

/// Parse a PEP 440 version, as `packaging.version.Version` does under the
/// interpreter's integer limit `limit`: a text that is no version is invalid
/// however long its numbers are, and a version with a number longer than the
/// limit allows is refused apart.
pub(crate) fn version(text: &str, limit: Limit) -> Result<Version, Refusal> {
    let parsed = parse(text, Grammar::Version).ok_or(Refusal::Invalid)?;
    limit.check(parsed.longest)?;
    Ok(parsed.version)
}

/// Print a version's normalized text, as `str(Version)` does under the
/// interpreter's integer limit `limit`: CPython converts an integer of more
/// than [`THRESHOLD`] digits back to text only within the limit too, counting
/// its significant digits. A version whose number the limit refuses, or that
/// needs the limit while it is unread, does not print.
///
/// CPython checks an integer of more than about 9,000 digits only coarsely on
/// its way back to text, which this does not reproduce. That matters only
/// when the limit was lowered after the version was read; under one limit, a
/// version that converts to an integer prints, having no more significant
/// digits than it was written with, and no observation of the Python host
/// changes the limit.
pub(crate) fn printed(version: &Version, limit: Limit) -> Result<String, Refusal> {
    limit.check(version.printed_digits())?;
    Ok(version.to_string())
}

/// The smallest version there is, `0.dev0`.
fn minimum() -> Version {
    Version::release(Number::zero(), vec![Number::zero()]).with_development(Number::zero())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Compatible,
    Equal,
    NotEqual,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    Arbitrary,
}

/// One version specifier: what `packaging.specifiers.Specifier` accepts.
#[derive(Debug)]
pub(crate) struct Specifier {
    operator: Operator,
    /// The version text after the operator, without surrounding white space.
    text: String,
    /// The parsed version with the digits of its longest number, absent for
    /// `===`. `packaging` converted the numbers only when a comparison read
    /// the version, after its candidate.
    version: Option<(Version, usize)>,
    wildcard: bool,
}

/// Parse a specifier, an operator directly followed by a version text, as
/// `packaging.specifiers.Specifier` does. `None` when it is no specifier.
pub(crate) fn specifier(text: &str) -> Option<Specifier> {
    let text = text.trim_matches(is_python_space);
    let operators = [
        ("===", Operator::Arbitrary),
        ("~=", Operator::Compatible),
        ("==", Operator::Equal),
        ("!=", Operator::NotEqual),
        ("<=", Operator::LessEqual),
        (">=", Operator::GreaterEqual),
        ("<", Operator::Less),
        (">", Operator::Greater),
    ];
    let (spelling, operator) = operators
        .iter()
        .find(|(spelling, _)| text.starts_with(spelling))?;
    let rest = text[spelling.len()..].trim_matches(is_python_space);
    if *operator == Operator::Arbitrary {
        // Any text without white space, a semicolon or a closing parenthesis.
        return (!rest.contains(|c: char| is_python_space(c) || c == ';' || c == ')')).then(|| {
            Specifier {
                operator: *operator,
                text: rest.to_owned(),
                version: None,
                wildcard: false,
            }
        });
    }
    let grammar = match operator {
        Operator::Equal | Operator::NotEqual => Grammar::Equality,
        Operator::Compatible => Grammar::Compatible,
        _ => Grammar::Ordered,
    };
    let parsed = parse(rest, grammar)?;
    Some(Specifier {
        operator: *operator,
        text: rest.to_owned(),
        version: Some((parsed.version, parsed.longest)),
        wildcard: parsed.wildcard,
    })
}

impl Specifier {
    /// Whether the specifier contains `candidate`, pre-releases included, as
    /// `Specifier.contains(candidate, prereleases=True)` answers. A candidate
    /// that is no version is contained only by an `===` that spells it.
    /// Under the interpreter's integer limit `limit`, a number the limit
    /// refuses, or one that needs the limit while it is unread, refuses the
    /// comparison: the candidate's, which is read first, or else the
    /// specifier's, which a candidate that is no version leaves unread. The
    /// refusal is never [`Refusal::Invalid`].
    pub(crate) fn contains(&self, candidate: &str, limit: Limit) -> Result<bool, Refusal> {
        let Some((version, longest)) = &self.version else {
            return Ok(candidate.to_lowercase() == self.text.to_lowercase());
        };
        let candidate = match self::version(candidate, limit) {
            Ok(candidate) => candidate,
            Err(Refusal::Invalid) => return Ok(false),
            Err(refusal) => return Err(refusal),
        };
        limit.check(*longest)?;
        let candidate = &candidate;
        Ok(match self.operator {
            Operator::Equal | Operator::NotEqual if self.wildcard => {
                let lower = Version::release(version.epoch.clone(), version.release.clone())
                    .with_development(Number::zero());
                let upper = Version::next_prefix(&version.epoch, &version.release);
                let within = &lower <= candidate && candidate < &upper;
                within == (self.operator == Operator::Equal)
            }
            Operator::Equal | Operator::NotEqual => {
                // Without its own local label, a version matches each of its
                // local variants.
                let equal = if self.text.contains('+') {
                    candidate == version
                } else {
                    version.same_public(candidate)
                };
                equal == (self.operator == Operator::Equal)
            }
            Operator::GreaterEqual => candidate >= version,
            Operator::LessEqual => candidate <= version || version.same_public(candidate),
            Operator::Greater => {
                if let Some(dev) = &version.dev {
                    candidate >= &version.with_development(dev.successor())
                } else if let Some(post) = &version.post {
                    let next = Version {
                        post: Some(post.successor()),
                        ..version.clone()
                    };
                    candidate >= &next.with_development(Number::zero())
                } else {
                    // Neither this version's local variants nor its
                    // post-releases are greater than it.
                    candidate > version
                        && !(version.same_base(candidate) && candidate.pre == version.pre)
                }
            }
            Operator::Less => {
                // Unless this version is a pre-release, its pre-releases are
                // not less than it.
                let bound = if version.is_prerelease() {
                    version.clone()
                } else {
                    version.with_development(Number::zero())
                };
                bound > minimum() && candidate < &bound
            }
            Operator::Compatible => {
                let prefix = &version.release[..version.release.len() - 1];
                candidate >= version && candidate < &Version::next_prefix(&version.epoch, prefix)
            }
            Operator::Arbitrary => unreachable!("an arbitrary specifier holds no version"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPython's default limit, read.
    const DEFAULT: Limit = Limit::Read(Some(DIGITS));

    /// A version read under the default limit.
    fn version(text: &str) -> Result<Version, Refusal> {
        super::version(text, DEFAULT)
    }

    fn normalized(text: &str) -> Option<String> {
        version(text).ok().map(|version| version.to_string())
    }

    #[test]
    fn versions_parse_and_print_their_normalized_form() {
        for (text, expected) in [
            ("1.0", "1.0"),
            ("1.0.0", "1.0.0"),
            ("01.002.0", "1.2.0"),
            ("v1.0", "1.0"),
            ("V1.0", "1.0"),
            ("  1.0\t", "1.0"),
            ("\u{1f}1.0\u{a0}", "1.0"),
            ("1!2.0", "1!2.0"),
            ("0!2.0", "2.0"),
            ("1.0a", "1.0a0"),
            ("1.0alpha1", "1.0a1"),
            ("1.0-A.1", "1.0a1"),
            ("1.0b2", "1.0b2"),
            ("1.0beta", "1.0b0"),
            ("1.0c1", "1.0rc1"),
            ("1.0pre1", "1.0rc1"),
            ("1.0preview_2", "1.0rc2"),
            ("1.0RC1", "1.0rc1"),
            ("1.0-1", "1.0.post1"),
            ("1.0.post", "1.0.post0"),
            ("1.0-r-3", "1.0.post3"),
            ("1.0rev4", "1.0.post4"),
            ("1.0.post.", "1.0.post0"),
            ("1.0a.post1", "1.0a0.post1"),
            ("1.0-dev1", "1.0.dev1"),
            ("1.0.dev", "1.0.dev0"),
            ("1.0a1.post2.dev3", "1.0a1.post2.dev3"),
            ("1.0+Local.07-x_y", "1.0+local.7.x.y"),
            ("1.0+abc.", ""),
            ("1.0+", ""),
            ("1.0.", ""),
            (".1", ""),
            ("1..2", ""),
            ("", ""),
            ("1.0 a1", ""),
            ("1.0prev", ""),
            ("1.0+a+b", ""),
            ("1.0-", ""),
            ("x1.0", ""),
            ("1.0.x", ""),
            ("1.0a1a1", ""),
            ("1.0\u{2160}", ""),
            ("99999999999999999999999.1", "99999999999999999999999.1"),
        ] {
            let expected = (!expected.is_empty()).then(|| expected.to_owned());
            assert_eq!(normalized(text), expected, "{text:?}");
        }
    }

    #[test]
    fn versions_order_as_pep_440_orders_them() {
        let ordered = [
            "0.dev0",
            "1.0.dev1",
            "1.0a1.dev1",
            "1.0a1",
            "1.0a1.post1.dev1",
            "1.0a1.post1",
            "1.0b1",
            "1.0rc1",
            "1.0",
            "1.0+abc",
            "1.0+abc.1",
            "1.0+1",
            "1.0+2",
            "1.0+10",
            "1.0.post1.dev1",
            "1.0.post1",
            "1.0.post2",
            "1.0.1",
            "1.1.dev1",
            "1.1",
            "2!0.1",
        ];
        for (index, left) in ordered.iter().enumerate() {
            for (other, right) in ordered.iter().enumerate() {
                let (left, right) = (version(left).unwrap(), version(right).unwrap());
                assert_eq!(left.cmp(&right), index.cmp(&other), "{left} {right}");
            }
        }
        assert_eq!(version("1.0").unwrap(), version("1.0.0.0").unwrap());
        assert_eq!(version("1.0+A").unwrap(), version("1.0+a").unwrap());
        assert_eq!(version("1.0+01").unwrap(), version("1.0+1").unwrap());
        assert_ne!(version("1.0+1").unwrap(), version("1.0").unwrap());
        assert_eq!(
            version("123456789012345678901").unwrap(),
            version("0123456789012345678901.0").unwrap()
        );
    }

    #[test]
    fn numbers_count_on_without_a_bound() {
        for (number, next) in [("0", "1"), ("8", "9"), ("9", "10"), ("1999", "2000")] {
            assert_eq!(Number::new(number).successor(), Number::new(next));
        }
    }

    fn contains(specifier_text: &str, candidate: &str) -> Option<Result<bool, Refusal>> {
        specifier(specifier_text).map(|specifier| specifier.contains(candidate, DEFAULT))
    }

    #[test]
    fn specifiers_accept_what_each_operator_admits() {
        for valid in [
            "==1.0",
            "== 1.0 ",
            "==1.0.*",
            "==1.0+local",
            "!=1.0.*",
            "~=1.0",
            "~=1.0a1",
            ">=1.0.post1",
            "<1!2",
            "===anything",
            "===",
            "=== spaced ",
        ] {
            assert!(specifier(valid).is_some(), "{valid:?}");
        }
        for invalid in [
            "~=1",
            "~=1.0+local",
            "~=1.0.*",
            "<1.0+local",
            ">=1.0.*",
            "==1.0.*.1",
            "==1.0a1.*",
            "==1.0 .*",
            "=1.0",
            "in1.0",
            "===a b",
            "===a;",
            "===a)",
            "==",
            "<",
        ] {
            assert!(specifier(invalid).is_none(), "{invalid:?}");
        }
    }

    #[test]
    fn specifiers_contain_as_packaging_matches_with_pre_releases() {
        for (specifier_text, candidate, expected) in [
            ("==3.12.*", "3.12.8", true),
            ("==3.12.*", "3.12", true),
            ("==3.12.*", "3.12.0rc1", true),
            ("==3.12.*", "3.13.0", false),
            ("==3.12.*", "3.12.1+local", true),
            ("!=3.12.*", "3.13.0", true),
            ("!=3.12.*", "3.12.4", false),
            ("==1.0", "1.0.0", true),
            ("==1.0", "1.0+local", true),
            ("==1.0+local", "1.0", false),
            ("==1.0+local", "1.0+LOCAL", true),
            ("!=1.0", "1.0+local", false),
            ("!=1.0", "1.0.post1", true),
            (">=3.13", "3.13.0", true),
            (">=3.13", "3.13.0rc1", false),
            (">=3.13", "3.12.9", false),
            ("<3.13", "3.12.9", true),
            ("<3.13", "3.13.0rc1", false),
            ("<3.13", "3.13.0.dev1", false),
            ("<3.13rc1", "3.13.0b1", true),
            ("<3.13.post1", "3.13", true),
            ("<3.13.post1", "3.13a1", true),
            ("<0", "0.dev0", false),
            ("<0.0.dev0", "0.dev0", false),
            ("<=3.13", "3.13+local", true),
            ("<=3.13", "3.13.post1", false),
            (">3.13", "3.13.post1", false),
            (">3.13", "3.13+local", false),
            (">3.13", "3.13.1", true),
            (">3.13", "3.14.0a1", true),
            (">3.13.post1", "3.13.post1+local", false),
            (">3.13.post1", "3.13.post2.dev0", true),
            (">3.13.dev1", "3.13.dev2", true),
            (">3.13.dev1", "3.13.dev1+local", false),
            ("~=2.2", "2.9", true),
            ("~=2.2", "3.0", false),
            ("~=2.2", "2.2a1", false),
            ("~=2.2.0", "2.3", false),
            ("~=1.4.5a4", "1.4.5", true),
            ("===3.12", "3.12", true),
            ("===3.12", "3.12.0", false),
            ("===ABC", "abc", true),
            ("==3.12", "not a version", false),
            ("===", "", true),
        ] {
            assert_eq!(
                contains(specifier_text, candidate),
                Some(Ok(expected)),
                "{specifier_text} {candidate}"
            );
        }
    }

    #[test]
    fn a_version_number_past_the_integer_limit_is_refused_apart() {
        let at = "1".repeat(DIGITS);
        let past = "1".repeat(DIGITS + 1);
        let zeros = format!("{}1", "0".repeat(DIGITS));
        for text in [
            at.clone(),
            format!("{at}!1.{at}a{at}.post{at}.dev{at}+{at}"),
            format!("1.0+a{past}"),
        ] {
            assert!(version(&text).is_ok(), "{}", text.len());
        }
        for text in [
            past.clone(),
            format!("v{past}"),
            format!(" {past} "),
            format!("{past}!1"),
            format!("1.{past}"),
            format!("1.0a{past}"),
            format!("1.0.post{past}"),
            format!("1.0-{past}"),
            format!("1.0.dev{past}"),
            format!("1.0+{past}"),
            format!("1.0+abc.{past}"),
            zeros.clone(),
        ] {
            assert_eq!(
                version(&text),
                Err(Refusal::Digits {
                    digits: DIGITS + 1,
                    limit: DIGITS
                }),
                "{}",
                &text[..10]
            );
        }
        // The grammar decides first: a text that is no version is invalid
        // however long its numbers are.
        for text in [format!("{past}."), format!("{past}x"), format!("1..{past}")] {
            assert_eq!(version(&text), Err(Refusal::Invalid));
        }
        // The longest number is the one reported.
        assert_eq!(
            version(&format!("{}.{past}", "2".repeat(DIGITS + 2))),
            Err(Refusal::Digits {
                digits: DIGITS + 2,
                limit: DIGITS
            })
        );
    }

    #[test]
    fn a_comparison_meets_a_number_past_the_limit_only_when_it_reads_it() {
        let past = "1".repeat(DIGITS + 1);
        let at = "1".repeat(DIGITS);
        // The specifier is one; the candidate is read first.
        assert_eq!(
            contains(&format!("=={past}"), "3.12"),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        assert_eq!(
            contains("==3.12", &past),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        assert_eq!(
            contains(&format!("=={past}"), &past),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        assert_eq!(
            contains(&format!("=={past}.*"), "3.12"),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        assert_eq!(
            contains(&format!("~=1.{past}"), "3.12"),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        assert_eq!(
            contains(&format!("<{past}"), "3.12"),
            Some(Err(Refusal::Digits {
                digits: DIGITS + 1,
                limit: DIGITS
            }))
        );
        // A candidate that is no version leaves the specifier's number unread,
        // and `===` reads no number.
        assert_eq!(
            contains(&format!("=={past}"), "6.8.0-generic"),
            Some(Ok(false))
        );
        assert_eq!(contains(&format!("==={past}"), "3.12"), Some(Ok(false)));
        assert_eq!(contains(&format!("==={past}"), &past), Some(Ok(true)));
        assert_eq!(contains(&format!("=={at}"), "3.12"), Some(Ok(false)));
        assert_eq!(contains(&format!("=={at}"), &at), Some(Ok(true)));
    }

    #[test]
    fn the_interpreter_s_own_limit_decides_past_the_threshold() {
        let within = "1".repeat(THRESHOLD);
        let beyond = "1".repeat(THRESHOLD + 1);
        // Up to the threshold, CPython converts whatever its limit, so the
        // limit is not needed; past it, an unread limit is.
        assert!(super::version(&within, Limit::Unread).is_ok());
        assert_eq!(super::version(&beyond, Limit::Unread), Err(Refusal::Unread));
        assert_eq!(
            super::version(&format!("{beyond}x"), Limit::Unread),
            Err(Refusal::Invalid)
        );
        for (limit, digits, converts) in [
            (Some(THRESHOLD), THRESHOLD, true),
            (Some(THRESHOLD), THRESHOLD + 1, false),
            (Some(DIGITS), DIGITS + 1, false),
            (Some(10_000), DIGITS + 1, true),
            (Some(10_000), 10_001, false),
            (None, 10_001, true),
        ] {
            let text = format!("1.{}", "0".repeat(digits));
            let read = super::version(&text, Limit::Read(limit));
            if converts {
                assert!(read.is_ok(), "{limit:?} {digits}");
            } else {
                let limit = limit.expect("a limit refuses");
                assert_eq!(
                    read,
                    Err(Refusal::Digits { digits, limit }),
                    "{limit} {digits}"
                );
            }
        }
        // A specifier's own number needs the limit only when a comparison
        // reads it, after a candidate that is a version.
        let bound = specifier(&format!("=={beyond}")).unwrap();
        assert_eq!(bound.contains("not a version", Limit::Unread), Ok(false));
        assert_eq!(bound.contains("3.12", Limit::Unread), Err(Refusal::Unread));
        assert_eq!(bound.contains("3.12", Limit::Read(None)), Ok(false));
        assert_eq!(bound.contains(&beyond, Limit::Read(None)), Ok(true));
        assert_eq!(
            bound.contains("3.12", Limit::Read(Some(THRESHOLD))),
            Err(Refusal::Digits {
                digits: THRESHOLD + 1,
                limit: THRESHOLD
            })
        );
        assert_eq!(
            specifier("==3.12")
                .unwrap()
                .contains(&beyond, Limit::Unread),
            Err(Refusal::Unread)
        );
    }
}
