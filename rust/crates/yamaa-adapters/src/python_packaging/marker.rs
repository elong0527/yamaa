//! PEP 508 environment markers, as Python's `packaging` 26 parses them and
//! evaluates a uv lock's `resolution-markers` (its "lock_file" context).
use super::version::specifier;
use std::collections::BTreeMap;

/// A marker that cannot be parsed or evaluated. Every such marker refuses its
/// lock alike, so no reason is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Invalid;

/// The values PEP 508 names, which the Python host reads from `sys`,
/// `platform` and `os`.
const NAMES: [&str; 11] = [
    "implementation_name",
    "implementation_version",
    "os_name",
    "platform_machine",
    "platform_release",
    "platform_system",
    "platform_version",
    "python_full_version",
    "platform_python_implementation",
    "python_version",
    "sys_platform",
];

/// The variables that comparisons treat as versions: a comparison matches by
/// version specifier when its operator and right side make one, and compares
/// text otherwise.
const VERSIONS: [&str; 4] = [
    "implementation_version",
    "platform_release",
    "python_full_version",
    "python_version",
];

/// The marker environment of a lock: the host's values of `NAMES`, with the
/// sets `extras` and `dependency_groups` empty and `extra` undefined.
pub(crate) struct Environment {
    values: BTreeMap<String, String>,
}

enum Value<'a> {
    Text(&'a str),
    /// `extras` or `dependency_groups`, which are empty in a lock.
    EmptySet,
}

impl Environment {
    pub(crate) fn new(mut values: BTreeMap<String, String>) -> Self {
        values.retain(|name, _| NAMES.contains(&name.as_str()));
        // An interpreter built from an untagged checkout reports a full
        // version that ends in `+`; packaging reads it as a local version.
        if let Some(full) = values.get_mut("python_full_version") {
            if full.ends_with('+') {
                full.push_str("local");
            }
        }
        Self { values }
    }

    fn lookup(&self, name: &str) -> Result<Value<'_>, Invalid> {
        match name {
            "extras" | "dependency_groups" => Ok(Value::EmptySet),
            _ => self
                .values
                .get(name)
                .map(|value| Value::Text(value))
                .ok_or(Invalid),
        }
    }
}

#[derive(Debug)]
enum Operand {
    /// A variable, by its normalized name.
    Variable(&'static str),
    Literal(String),
}

impl Operand {
    /// The variable's name or the literal's text.
    fn text(&self) -> &str {
        match self {
            Self::Variable(name) => name,
            Self::Literal(text) => text,
        }
    }
}

#[derive(Debug)]
struct Comparison {
    left: Operand,
    operator: &'static str,
    right: Operand,
}

#[derive(Debug)]
enum Element {
    Comparison(Comparison),
    Group(Vec<Element>),
    And,
    Or,
}

/// A parsed marker.
#[derive(Debug)]
pub(crate) struct Marker(Vec<Element>);

/// How deeply parentheses may nest. Python refused a marker that exhausted
/// its recursion limit; this bound keeps the parser's stack small.
const DEPTH: usize = 100;

/// The variable spellings, each with its normalized name.
const VARIABLES: [(&str, &str); 20] = [
    ("python_version", "python_version"),
    ("python_full_version", "python_full_version"),
    ("os_name", "os_name"),
    ("os.name", "os_name"),
    ("sys_platform", "sys_platform"),
    ("sys.platform", "sys_platform"),
    ("platform_release", "platform_release"),
    ("platform_system", "platform_system"),
    ("platform_version", "platform_version"),
    ("platform.version", "platform_version"),
    ("platform_machine", "platform_machine"),
    ("platform.machine", "platform_machine"),
    (
        "platform_python_implementation",
        "platform_python_implementation",
    ),
    (
        "platform.python_implementation",
        "platform_python_implementation",
    ),
    ("python_implementation", "platform_python_implementation"),
    ("implementation_name", "implementation_name"),
    ("implementation_version", "implementation_version"),
    ("extras", "extras"),
    ("extra", "extra"),
    ("dependency_groups", "dependency_groups"),
];

/// A character of Python's `\w`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.source[self.at..]
    }

    /// Spaces and tabs.
    fn whitespace(&mut self) -> bool {
        let length = self
            .rest()
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        self.at += length;
        length > 0
    }

    /// Whether `spelling` follows, starting and ending at word boundaries.
    fn word(&mut self, spelling: &str) -> bool {
        let before = self.source[..self.at].chars().next_back();
        let rest = self.rest();
        if before.is_some_and(is_word)
            || !rest.starts_with(spelling)
            || rest[spelling.len()..].chars().next().is_some_and(is_word)
        {
            return false;
        }
        self.at += spelling.len();
        true
    }

    fn marker(&mut self, depth: usize) -> Result<Vec<Element>, Invalid> {
        let mut elements = vec![self.atom(depth)?];
        loop {
            let logic = if self.word("or") {
                Element::Or
            } else if self.word("and") {
                Element::And
            } else {
                return Ok(elements);
            };
            elements.push(logic);
            elements.push(self.atom(depth)?);
        }
    }

    fn atom(&mut self, depth: usize) -> Result<Element, Invalid> {
        self.whitespace();
        let atom = if self.rest().starts_with('(') {
            if depth == DEPTH {
                return Err(Invalid);
            }
            self.at += 1;
            self.whitespace();
            let inner = self.marker(depth + 1)?;
            self.whitespace();
            if !self.rest().starts_with(')') {
                return Err(Invalid);
            }
            self.at += 1;
            Element::Group(inner)
        } else {
            self.whitespace();
            let left = self.operand()?;
            self.whitespace();
            let operator = self.operator()?;
            self.whitespace();
            let right = self.operand()?;
            Element::Comparison(Comparison {
                left,
                operator,
                right,
            })
        };
        self.whitespace();
        Ok(atom)
    }

    fn operand(&mut self) -> Result<Operand, Invalid> {
        for (spelling, name) in VARIABLES {
            if self.word(spelling) {
                return Ok(Operand::Variable(name));
            }
        }
        let quote = self
            .rest()
            .chars()
            .next()
            .filter(|c| matches!(c, '\'' | '"'))
            .ok_or(Invalid)?;
        let body = &self.rest()[1..];
        let end = body.find(quote).ok_or(Invalid)?;
        let text = &body[..end];
        self.at += end + 2;
        // packaging read the text as a Python string literal. PEP 508 has no
        // escapes and uv writes none, so a backslash refuses the marker, as a
        // line break or a NUL did there.
        if text.contains(['\\', '\n', '\r', '\0']) {
            return Err(Invalid);
        }
        Ok(Operand::Literal(text.to_owned()))
    }

    fn operator(&mut self) -> Result<&'static str, Invalid> {
        if self.word("in") {
            return Ok("in");
        }
        if self.word("not") {
            if self.whitespace() && self.word("in") {
                return Ok("not in");
            }
            return Err(Invalid);
        }
        for operator in ["===", "==", "~=", "!=", "<=", ">=", "<", ">"] {
            if self.rest().starts_with(operator) {
                self.at += operator.len();
                return Ok(operator);
            }
        }
        Err(Invalid)
    }
}

/// Parse a marker, as `packaging.markers.Marker` does.
pub(crate) fn parse(source: &str) -> Result<Marker, Invalid> {
    let mut parser = Parser { source, at: 0 };
    let elements = parser.marker(0)?;
    if parser.at == source.len() {
        Ok(Marker(elements))
    } else {
        Err(Invalid)
    }
}

impl Marker {
    /// Evaluate the marker in `environment`. Every comparison is evaluated,
    /// so one that cannot be fails the marker whatever the others give;
    /// `and` binds more tightly than `or`.
    pub(crate) fn evaluate(&self, environment: &Environment) -> Result<bool, Invalid> {
        evaluate(&self.0, environment)
    }
}

fn evaluate(elements: &[Element], environment: &Environment) -> Result<bool, Invalid> {
    let mut groups = vec![true];
    for element in elements {
        let value = match element {
            Element::Or => {
                groups.push(true);
                continue;
            }
            Element::And => continue,
            Element::Group(inner) => evaluate(inner, environment)?,
            Element::Comparison(comparison) => comparison.evaluate(environment)?,
        };
        let group = groups.last_mut().expect("one group at least");
        *group = *group && value;
    }
    Ok(groups.into_iter().any(|group| group))
}

impl Comparison {
    fn evaluate(&self, environment: &Environment) -> Result<bool, Invalid> {
        // The variable on the left, or else the right side, names the value
        // that the environment supplies.
        let (name, left, right) = match &self.left {
            Operand::Variable(name) => (
                *name,
                environment.lookup(name)?,
                Value::Text(self.right.text()),
            ),
            Operand::Literal(text) => {
                let name = self.right.text();
                (name, Value::Text(text), environment.lookup(name)?)
            }
        };
        // A set can only be the right side of `in` or `not in`.
        let Value::Text(left) = left else {
            return Err(Invalid);
        };
        if VERSIONS.contains(&name) {
            if let Value::Text(right) = right {
                if let Some(specifier) = specifier(&format!("{}{right}", self.operator)) {
                    return Ok(specifier.contains(left));
                }
            }
        }
        // Text compares only for equality: an ordering comparison is false,
        // and `~=` and `===` compare only versions.
        match (self.operator, right) {
            ("in", Value::Text(right)) => Ok(right.contains(left)),
            ("not in", Value::Text(right)) => Ok(!right.contains(left)),
            ("in", Value::EmptySet) | ("<" | ">", _) => Ok(false),
            ("not in", Value::EmptySet) => Ok(true),
            ("<=" | "==" | ">=", Value::Text(right)) => Ok(left == right),
            ("<=" | "==" | ">=", Value::EmptySet) => Ok(false),
            ("!=", Value::Text(right)) => Ok(left != right),
            ("!=", Value::EmptySet) => Ok(true),
            _ => Err(Invalid),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linux() -> Environment {
        Environment::new(
            [
                ("implementation_name", "cpython"),
                ("implementation_version", "3.12.8"),
                ("os_name", "posix"),
                ("platform_machine", "x86_64"),
                ("platform_release", "6.8.0-generic"),
                ("platform_system", "Linux"),
                ("platform_version", "#1 SMP PREEMPT_DYNAMIC"),
                ("python_full_version", "3.12.8"),
                ("platform_python_implementation", "CPython"),
                ("python_version", "3.12"),
                ("sys_platform", "linux"),
            ]
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        )
    }

    fn evaluated(source: &str, environment: &Environment) -> Result<bool, Invalid> {
        parse(source)?.evaluate(environment)
    }

    #[test]
    fn uv_markers_select_by_platform_and_python() {
        let environment = linux();
        for (source, expected) in [
            ("sys_platform == 'linux'", true),
            ("sys_platform == 'win32'", false),
            ("sys_platform != 'win32'", true),
            ("python_full_version >= '3.13'", false),
            ("python_full_version < '3.13'", true),
            ("python_full_version == '3.12.*'", true),
            ("python_full_version == '3.14.*'", false),
            ("python_version >= '3.12'", true),
            (
                "platform_machine == 'x86_64' and sys_platform == 'linux'",
                true,
            ),
            (
                "(platform_machine == 'aarch64' and sys_platform == 'linux') or sys_platform == 'darwin'",
                false,
            ),
            ("implementation_name == 'pypy'", false),
            ("platform_python_implementation == 'CPython'", true),
            ("python_implementation == 'CPython'", true),
            ("os.name == 'posix' and sys.platform == 'linux'", true),
            ("platform.machine == \"x86_64\"", true),
            ("'linux' == sys_platform", true),
            ("'3.11' < python_version", true),
            ("'3.13' < python_version", false),
            ("'nux' in sys_platform", true),
            ("'win' not in sys_platform", true),
            ("'x' in extras", false),
            ("'x' not in dependency_groups", true),
            ("sys_platform == 'linux'and python_version > '3'", true),
            ("sys_platform=='linux'", true),
            ("  ( ( sys_platform == 'linux' ) )  ", true),
            ("sys_platform == 'win32' or sys_platform == 'linux' and python_version < '3'", false),
            ("sys_platform < 'z'", false),
            ("sys_platform <= 'linux'", true),
            ("platform_release >= '5.0'", false),
            ("platform_release == '6.8.0-generic'", true),
            ("python_version == '=3.12'", true),
            ("python_version < '=3.12'", true),
            ("'3.12' == 'python_version'", true),
            ("python_version == python_full_version", false),
            ("python_version in '3.12.8'", true),
        ] {
            assert_eq!(evaluated(source, &environment), Ok(expected), "{source}");
        }
    }

    #[test]
    fn markers_that_cannot_be_parsed_or_evaluated_are_invalid() {
        let environment = linux();
        for source in [
            "",
            "not a marker",
            "sys_platform",
            "sys_platform ==",
            "sys_platform = 'linux'",
            "sys_platform == linux",
            "sys_platform == 'linux",
            "(sys_platform == 'linux'",
            "sys_platform == 'linux')",
            "sys_platform == 'linux' and",
            "sys_platform == 'linux' andpython_version > '3'",
            "sys_platform == 'linux'\n",
            "platform.release == '6'",
            "sys_platform == 'lin\\x75x'",
            "sys_platform == 'li\nnux'",
            "sys_platform not  'linux'",
            "sys_platform notin 'linux'",
            "extra == 'cpu'",
            "extras == 'cpu'",
            "'a' == 'b'",
            "sys_platform ~= 'linux'",
            "sys_platform === 'linux'",
            "python_version ~= 'x'",
            "sys_platform == 'win32' or extra == 'cpu'",
            "python_versions == '3.12'",
        ] {
            assert_eq!(evaluated(source, &environment), Err(Invalid), "{source:?}");
        }
        let nested = format!(
            "{}sys_platform == 'linux'{}",
            "(".repeat(DEPTH),
            ")".repeat(DEPTH)
        );
        assert_eq!(evaluated(&nested, &environment), Ok(true));
        let deeper = format!("({nested})");
        assert_eq!(evaluated(&deeper, &environment), Err(Invalid));
    }

    #[test]
    fn an_untagged_interpreter_version_reads_as_a_local_version() {
        let mut values = BTreeMap::new();
        values.insert("python_full_version".to_owned(), "3.14.0+".to_owned());
        values.insert("extra".to_owned(), "cpu".to_owned());
        let environment = Environment::new(values);
        assert_eq!(
            evaluated("python_full_version >= '3.14'", &environment),
            Ok(true)
        );
        assert_eq!(
            evaluated("python_full_version == '3.14.0+local'", &environment),
            Ok(true)
        );
        // The host's values never define `extra`, nor a value PEP 508 does
        // not name.
        assert_eq!(evaluated("extra == 'cpu'", &environment), Err(Invalid));
        assert_eq!(
            evaluated("sys_platform == 'linux'", &environment),
            Err(Invalid)
        );
    }
}
