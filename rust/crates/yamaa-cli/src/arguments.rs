//! The command line's arguments (#3030 section 2.8, D12, D68), read with the
//! standard library alone.
//!
//! The grammar is `yamaa [--format text|json] COMMAND [ARGUMENTS]`,
//! `yamaa --version [--format text|json]` and `yamaa --help`. `--format` may
//! come before or after the command, and `--help` anywhere before `--` asks
//! for the usage whatever else is given. A flag that takes a value reads the
//! next argument, and `--` ends the flags, so a PATH may begin with `-`. A
//! command takes the positional arguments and flags its synopsis lists, each
//! flag at most once. Anything else is a usage error, which exits with
//! status 2.
//!
//! Here a value is checked only for its form: a count is a whole number, and
//! a name, section, time or file type is any text. The engine checks the rest
//! against its own vocabularies when the commands run.

use std::ffi::{OsStr, OsString};

/// What a flag takes after it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Takes {
    /// Nothing: the flag is a switch, such as `--fix`.
    Nothing,
    /// Any text, which the engine checks: a name, a section or a time.
    Text,
    /// A whole number, such as a bound of `read-odm`.
    Count,
}

#[derive(Clone, Copy)]
struct Flag {
    name: &'static str,
    takes: Takes,
}

const fn flag(name: &'static str, takes: Takes) -> Flag {
    Flag { name, takes }
}

const SPEC: Flag = flag("--spec", Takes::Text);
const SECTION: Flag = flag("--section", Takes::Text);
const CREATED: Flag = flag("--created", Takes::Text);
const FIX: Flag = flag("--fix", Takes::Nothing);
const ARCHIVE_MEMBER: Flag = flag("--archive-member", Takes::Text);
const MAX_ARCHIVE_MEMBERS: Flag = flag("--max-archive-members", Takes::Count);
const MAX_EXPANDED_BYTES: Flag = flag("--max-expanded-bytes", Takes::Count);
const MAX_XML_DEPTH: Flag = flag("--max-xml-depth", Takes::Count);

/// One command: its name, its synopsis, its positional arguments and its
/// flags.
pub(crate) struct Command {
    pub(crate) name: &'static str,
    /// The synopsis after `yamaa`, as `--help` prints it.
    pub(crate) synopsis: &'static str,
    /// The positional arguments in order; with `repeats`, the last may be
    /// given more than once.
    arguments: &'static [&'static str],
    repeats: bool,
    flags: &'static [Flag],
    /// Flags of which one call may give only one.
    exclusive: &'static [&'static str],
}

/// Every command, in the order of the design's table (#3030 section 2.8,
/// with D68's `--spec` and `--section`).
pub(crate) static COMMANDS: [Command; 8] = [
    Command {
        name: "check",
        synopsis: "check PATH [--spec NAME]",
        arguments: &["PATH"],
        repeats: false,
        flags: &[SPEC],
        exclusive: &[],
    },
    Command {
        name: "run",
        synopsis: "run PATH [--spec NAME | --section SECTION]",
        arguments: &["PATH"],
        repeats: false,
        flags: &[SPEC, SECTION],
        exclusive: &["--spec", "--section"],
    },
    Command {
        name: "submission",
        synopsis: "submission PATH [--section SECTION] [--created TIME]",
        arguments: &["PATH"],
        repeats: false,
        flags: &[SECTION, CREATED],
        exclusive: &[],
    },
    Command {
        name: "resolve",
        synopsis: "resolve PATH [--spec NAME]",
        arguments: &["PATH"],
        repeats: false,
        flags: &[SPEC],
        exclusive: &[],
    },
    Command {
        name: "read-odm",
        synopsis: "read-odm SOURCE OUT [--archive-member NAME] [--max-archive-members N] \
                   [--max-expanded-bytes N] [--max-xml-depth N]",
        arguments: &["SOURCE", "OUT"],
        repeats: false,
        flags: &[
            ARCHIVE_MEMBER,
            MAX_ARCHIVE_MEMBERS,
            MAX_EXPANDED_BYTES,
            MAX_XML_DEPTH,
        ],
        exclusive: &[],
    },
    Command {
        name: "lint",
        synopsis: "lint [--fix] PATH...",
        arguments: &["PATH"],
        repeats: true,
        flags: &[FIX],
        exclusive: &[],
    },
    Command {
        name: "schema",
        synopsis: "schema TYPE",
        arguments: &["TYPE"],
        repeats: false,
        flags: &[],
        exclusive: &[],
    },
    Command {
        name: "selftest",
        synopsis: "selftest",
        arguments: &[],
        repeats: false,
        flags: &[],
        exclusive: &[],
    },
];

impl Command {
    fn flag(&self, name: &str) -> Option<Flag> {
        self.flags.iter().copied().find(|flag| flag.name == name)
    }

    /// Refuse a missing or extra positional argument, and two flags that
    /// exclude each other.
    fn check(&self, positional: &[&OsString], given: &[&'static str]) -> Result<(), Usage> {
        if let Some(missing) = self.arguments.get(positional.len()) {
            return Err(usage(format!("{}: missing {missing}", self.name)));
        }
        if let Some(extra) = positional
            .get(self.arguments.len())
            .filter(|_| !self.repeats)
        {
            return Err(usage(format!(
                "{}: unexpected argument {}",
                self.name,
                extra.to_string_lossy()
            )));
        }
        if self
            .exclusive
            .iter()
            .filter(|name| given.contains(*name))
            .count()
            > 1
        {
            return Err(usage(format!(
                "{}: give {}, not both",
                self.name,
                self.exclusive.join(" or ")
            )));
        }
        Ok(())
    }
}

/// The output format `--format` selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Text,
    Json,
}

/// What a valid command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Invocation {
    /// `--help`: the usage.
    Help,
    /// `--version`: the engine information.
    Version(Format),
    /// A command, by name, with every argument checked.
    Command(&'static str, Format),
}

/// A usage error: why the command line is refused, with exit status 2.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Usage(pub(crate) String);

fn usage(message: impl Into<String>) -> Usage {
    Usage(message.into())
}

/// Refuse a flag given a second time.
fn once(given: bool, name: &str) -> Result<(), Usage> {
    if given {
        Err(usage(format!("{name} is given twice")))
    } else {
        Ok(())
    }
}

/// A whole number: decimal digits that fit in 64 bits.
fn is_count(value: &OsStr) -> bool {
    value.to_str().is_some_and(|text| {
        !text.is_empty()
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && text.parse::<u64>().is_ok()
    })
}

/// Read the arguments that follow the program name.
pub(crate) fn parse(arguments: &[OsString]) -> Result<Invocation, Usage> {
    let flags_end = arguments
        .iter()
        .position(|argument| argument == "--")
        .unwrap_or(arguments.len());
    if arguments[..flags_end]
        .iter()
        .any(|argument| argument == "--help")
    {
        return Ok(Invocation::Help);
    }
    let mut format = None;
    let mut version = false;
    let mut command: Option<&'static Command> = None;
    let mut positional = Vec::new();
    let mut given = Vec::new();
    let mut ended = false;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let text = argument.to_string_lossy();
        if ended || text == "-" || !text.starts_with('-') {
            match command {
                None => {
                    command = Some(
                        COMMANDS
                            .iter()
                            .find(|command| command.name == text)
                            .ok_or_else(|| usage(format!("unknown command {text}")))?,
                    );
                }
                Some(_) => positional.push(argument),
            }
            continue;
        }
        match &*text {
            "--" => ended = true,
            "--format" => {
                once(format.is_some(), "--format")?;
                let value = rest
                    .next()
                    .ok_or_else(|| usage("--format needs a value: text or json"))?;
                format = Some(match value.to_str() {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    _ => {
                        return Err(usage(format!(
                            "--format takes text or json, not {}",
                            value.to_string_lossy()
                        )))
                    }
                });
            }
            "--version" => {
                once(version, "--version")?;
                version = true;
            }
            name => {
                let Some(current) = command else {
                    return Err(
                        if COMMANDS.iter().any(|command| command.flag(name).is_some()) {
                            usage(format!("{name} must follow its command"))
                        } else {
                            usage(format!("unknown option {name}"))
                        },
                    );
                };
                let flag = current
                    .flag(name)
                    .ok_or_else(|| usage(format!("{}: unknown option {name}", current.name)))?;
                once(
                    given.contains(&flag.name),
                    &format!("{}: {name}", current.name),
                )?;
                given.push(flag.name);
                if flag.takes != Takes::Nothing {
                    let value = rest
                        .next()
                        .ok_or_else(|| usage(format!("{}: {name} needs a value", current.name)))?;
                    if flag.takes == Takes::Count && !is_count(value) {
                        return Err(usage(format!(
                            "{}: {name} takes a whole number, not {}",
                            current.name,
                            value.to_string_lossy()
                        )));
                    }
                }
            }
        }
    }
    let format = format.unwrap_or(Format::Text);
    if version {
        return match command {
            None => Ok(Invocation::Version(format)),
            Some(_) => Err(usage("--version takes no command")),
        };
    }
    let command = command.ok_or_else(|| usage("no command given"))?;
    command.check(&positional, &given)?;
    Ok(Invocation::Command(command.name, format))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(words: &[&str]) -> Result<Invocation, Usage> {
        parse(&words.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn the_format_may_come_before_or_after_the_command() {
        for words in [
            &["--format", "json", "check", "spec.yaml"][..],
            &["check", "--format", "json", "spec.yaml"],
            &["check", "spec.yaml", "--format", "json"],
        ] {
            assert_eq!(
                parsed(words),
                Ok(Invocation::Command("check", Format::Json)),
                "{words:?}"
            );
        }
        assert_eq!(
            parsed(&["check", "spec.yaml"]),
            Ok(Invocation::Command("check", Format::Text))
        );
        assert_eq!(
            parsed(&["--version"]),
            Ok(Invocation::Version(Format::Text))
        );
    }

    #[test]
    fn a_double_dash_ends_the_flags() {
        assert_eq!(
            parsed(&["lint", "--", "--fix", "-"]),
            Ok(Invocation::Command("lint", Format::Text))
        );
        // After `--`, `--help` is a PATH, not a request for the usage.
        assert_eq!(
            parsed(&["check", "--", "--help"]),
            Ok(Invocation::Command("check", Format::Text))
        );
        assert_eq!(
            parsed(&["check", "--", "a.yaml", "b.yaml"]),
            Err(Usage("check: unexpected argument b.yaml".into()))
        );
    }

    #[test]
    fn every_command_names_its_flags_in_its_synopsis() {
        for command in &COMMANDS {
            assert!(
                command.synopsis.starts_with(command.name),
                "{}",
                command.name
            );
            for flag in command.flags {
                assert!(command.synopsis.contains(flag.name), "{}", flag.name);
            }
            for argument in command.arguments {
                assert!(command.synopsis.contains(argument), "{argument}");
            }
        }
    }

    #[test]
    fn a_count_is_a_whole_number_that_fits_in_64_bits() {
        for (value, count) in [
            ("0", true),
            ("64", true),
            ("18446744073709551615", true),
            ("18446744073709551616", false),
            ("", false),
            ("+1", false),
            ("-1", false),
            ("1e3", false),
            (" 1", false),
        ] {
            assert_eq!(is_count(OsStr::new(value)), count, "{value:?}");
        }
    }
}
