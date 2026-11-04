//! The project functions of a standalone run: an interpreter's worker, which
//! the binary starts and talks to over its standard input and output.
//!
//! The binary runs on no interpreter, so a run that calls a project
//! function starts one, `python -m yamaa._worker` or
//! `Rscript -e 'yamaa:::.yamaa_worker()'`, the first time the engine asks
//! its host-functions port for anything, and keeps it until the port drops.
//! The worker is the host package's: it resolves each callable, runs each
//! call and reports what is installed with the same helpers the package's
//! own `run()` uses, so a function behaves as it does there. The worker's
//! standard error is the binary's, so a function's warnings reach the
//! terminal; its standard output carries the protocol, which a function
//! that writes there would corrupt, so the Python worker moves `sys.stdout`
//! to standard error, and an R function must not write to standard output.
//!
//! The protocol is lines of UTF-8 text ending in a line feed, each a request
//! of the binary or the worker's reply to it, in order, one reply per
//! request. Fields are separated by a tab, and every text field is
//! percent-encoded: a byte of `A-Za-z0-9-._~` as itself, any other as `%`
//! and two uppercase hexadecimal digits, so no field holds a tab, a line
//! feed or a separator of its own. A value is `-` for missing, or a type
//! letter, a colon and the value: `s:` and the text, `i:` and a decimal
//! integer, `f:` and a decimal float, or `inf`, `-inf` or `nan`, `b:` and
//! `true` or `false`, `d:` and `YYYY-MM-DD`, `t:` and
//! `YYYY-MM-DDTHH:MM:SS`.
//!
//! | Request | Reply |
//! |---|---|
//! | `info` | `ok`, the engine version of the installed package |
//! | `providers`, a module... | `ok`, then `module=distribution,...` for each module, in order |
//! | `standard_library`, a module | `ok`, `true` or `false` |
//! | `version`, a distribution | `ok`, then `missing`, `unreadable`, or `version` and the text |
//! | `environment` | `ok`, then `key=value` for each marker value |
//! | `integer_digits` | `ok`, the interpreter's limit, 0 for none |
//! | `verify`, a count, that many calls, then `package=version`... | `ok`, then `package\|reason\|expected\|actual` per finding, `-` for none |
//! | `bind`, a callable, its return type, its parameters... | `ok`, a handle |
//! | `invoke`, a handle, `name=value`... | `ok`, the value |
//! | `batch`, a handle, a row count, a parameter count; then one line per parameter, its name and one value per row | `ok`, one value per row |
//! | `exit` | `ok`, and the worker ends |
//!
//! Any request may be answered `error`, a class and a message, for an
//! exception the worker's code raised; `interrupt`, for the interpreter's
//! interrupt; or `rejected`, a reason and the type of a returned value, or
//! `-`, for a result that is no scalar of the declared type. `providers`,
//! `standard_library`, `version`, `environment` and `integer_digits` are the
//! Python worker's observations for the engine's uv lock verification;
//! `verify` is the R worker's renv verification. Each reply names the
//! engine's own failure: a raised exception, an interrupt, a rejected
//! result, or a lock finding; anything else the binary reads, and a worker
//! that cannot start or that ends, is an exception of the class `worker`.
//!
//! The binary's engine and the worker's package must be one engine version:
//! the lock pins yamaa because the engine is part of what the functions were
//! tested with (FUNC-3), so a declared lock's verification ends with the
//! worker's `info`, and a version that differs from the binary's is a lock
//! finding of the package `yamaa`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use yamaa::host::functions::{
    Argument, BatchArgument, CallError, FunctionError, FunctionHandle, FunctionIdentity,
    InstalledDistributions, InstalledVersion, LockContext, LockFinding, LockKind, LockReason,
    Signature,
};
use yamaa::scalar::{Date, DateTime, Scalar, ScalarRef};
use yamaa::{HostLanguage, ProjectFunctions};

/// The worker of one language, started when first asked, on the interpreter
/// the user named or the first of the language's defaults found on PATH.
pub struct Worker {
    language: HostLanguage,
    interpreter: Option<String>,
    connection: Mutex<Option<Connection>>,
}

/// A started worker: its process, and its standard input and output.
struct Connection {
    child: Child,
    requests: BufWriter<ChildStdin>,
    replies: BufReader<ChildStdout>,
}

impl Worker {
    pub fn new(language: HostLanguage, interpreter: Option<String>) -> Self {
        Self {
            language,
            interpreter,
            connection: Mutex::new(None),
        }
    }

    /// The interpreters tried in order when none is named: `python3` then
    /// `python` on POSIX, where `python` is often Python 2 or absent, and
    /// the other way round on Windows; `Rscript` for R.
    fn defaults(&self) -> &'static [&'static str] {
        match (self.language, cfg!(windows)) {
            (HostLanguage::Python, false) => &["python3", "python"],
            (HostLanguage::Python, true) => &["python", "python3"],
            (HostLanguage::R, _) => &["Rscript"],
            _ => &[],
        }
    }

    /// The arguments that start the language's worker in an interpreter.
    fn arguments(&self) -> &'static [&'static str] {
        match self.language {
            HostLanguage::Python => &["-m", "yamaa._worker"],
            HostLanguage::R => &["-e", "yamaa:::.yamaa_worker()"],
            _ => &[],
        }
    }

    /// Start the worker: the named interpreter, or the first default that
    /// starts.
    fn start(&self) -> Result<Connection, FunctionError> {
        let candidates: Vec<&str> = match &self.interpreter {
            Some(interpreter) => vec![interpreter.as_str()],
            None => self.defaults().to_vec(),
        };
        let mut failures = Vec::new();
        for interpreter in &candidates {
            let spawned = Command::new(interpreter)
                .args(self.arguments())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn();
            match spawned {
                Ok(mut child) => {
                    let requests = child.stdin.take().expect("piped stdin");
                    let replies = child.stdout.take().expect("piped stdout");
                    return Ok(Connection {
                        child,
                        requests: BufWriter::new(requests),
                        replies: BufReader::new(replies),
                    });
                }
                Err(error) => failures.push(format!("{interpreter}: {error}")),
            }
        }
        Err(worker_failure(format!(
            "cannot start the {} worker: {}",
            self.language.as_str(),
            failures.join("; ")
        )))
    }

    /// Send one request, with `lines` more lines after it, and read its
    /// reply's fields; a worker that cannot be reached is an exception of
    /// the class `worker`.
    fn exchange(&self, request: &str, lines: &[String]) -> Result<Vec<String>, FunctionError> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| worker_failure("the worker's connection is poisoned".to_owned()))?;
        if guard.is_none() {
            *guard = Some(self.start()?);
        }
        let connection = guard.as_mut().expect("started");
        let send = |connection: &mut Connection| -> std::io::Result<String> {
            connection.requests.write_all(request.as_bytes())?;
            connection.requests.write_all(b"\n")?;
            for line in lines {
                connection.requests.write_all(line.as_bytes())?;
                connection.requests.write_all(b"\n")?;
            }
            connection.requests.flush()?;
            let mut reply = String::new();
            let read = connection.replies.read_line(&mut reply)?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the worker ended",
                ));
            }
            Ok(reply)
        };
        let reply = send(connection).map_err(|error| {
            worker_failure(format!(
                "the {} worker did not answer {}: {error}",
                self.language.as_str(),
                request.split('\t').next().unwrap_or_default()
            ))
        })?;
        let reply = reply.trim_end_matches(['\n', '\r']);
        Ok(reply.split('\t').map(str::to_owned).collect())
    }

    /// Send `request` and take its `ok` reply's fields; any other reply is
    /// the failure it names.
    fn ask(&self, request: &str, lines: &[String]) -> Result<Vec<String>, FunctionError> {
        let fields = self.exchange(request, lines)?;
        match fields.first().map(String::as_str) {
            Some("ok") => Ok(fields[1..].to_vec()),
            Some("error") => Err(FunctionError::Exception {
                class: decode(fields.get(1).map_or("", String::as_str))?,
                message: decode(fields.get(2).map_or("", String::as_str))?,
                payload: None,
            }),
            Some("interrupt") => Err(FunctionError::Interrupted { payload: None }),
            Some("rejected") => Err(FunctionError::Rejected {
                reason: decode(fields.get(1).map_or("", String::as_str))?,
                returned: match fields.get(2).map(String::as_str) {
                    None | Some("-") => None,
                    Some(returned) => Some(decode(returned)?),
                },
            }),
            _ => Err(worker_failure(format!(
                "the {} worker answered {} with no reply of the protocol: {}",
                self.language.as_str(),
                request.split('\t').next().unwrap_or_default(),
                fields.join("\t")
            ))),
        }
    }

    /// The engine version of the worker's installed package.
    fn engine_version(&self) -> Result<String, FunctionError> {
        let fields = self.ask("info", &[])?;
        decode(fields.first().map_or("", String::as_str))
    }

    /// The binary's engine and the worker's package must be one engine
    /// version (FUNC-3): a difference is a lock finding of `yamaa`, whose
    /// expected version is the worker's and whose actual is the binary's.
    fn same_engine(&self) -> Result<(), FunctionError> {
        let installed = self.engine_version()?;
        if installed == yamaa::VERSION {
            return Ok(());
        }
        Err(FunctionError::Lock(vec![LockFinding::new(
            "yamaa".to_owned(),
            LockReason::VersionMismatch,
            vec![installed],
            Some(yamaa::VERSION.to_owned()),
        )]))
    }
}

impl Drop for Worker {
    /// End the worker: ask it to exit, then make sure it has.
    fn drop(&mut self) {
        if let Ok(mut guard) = self.connection.lock() {
            if let Some(mut connection) = guard.take() {
                let _ = connection.requests.write_all(b"exit\n");
                let _ = connection.requests.flush();
                let _ = connection.child.kill();
                let _ = connection.child.wait();
            }
        }
    }
}

/// A failure of the worker itself, which the engine reports as a host
/// exception of the class `worker`. The engine keeps a host failure of its
/// lock verification opaque, so the reason is written to standard error
/// too, as `yamaa: <message>`, where the binary's own refusals go.
fn worker_failure(message: String) -> FunctionError {
    let _ = writeln!(std::io::stderr(), "yamaa: {message}");
    FunctionError::Exception {
        class: "worker".to_owned(),
        message,
        payload: None,
    }
}

impl InstalledDistributions for Worker {
    fn providers(
        &self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
        let mut request = String::from("providers");
        for module in modules {
            request.push('\t');
            request.push_str(&encode(module));
        }
        let mut providers = BTreeMap::new();
        for field in self.ask(&request, &[])? {
            let (module, names) = field.split_once('=').ok_or_else(|| {
                worker_failure(format!("a provider entry names no module: {field}"))
            })?;
            let names = names
                .split(',')
                .filter(|name| !name.is_empty())
                .map(decode)
                .collect::<Result<Vec<_>, _>>()?;
            providers.insert(decode(module)?, names);
        }
        Ok(providers)
    }

    fn standard_library(&self, module: &str) -> Result<bool, FunctionError> {
        let fields = self.ask(&format!("standard_library\t{}", encode(module)), &[])?;
        match fields.first().map(String::as_str) {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(FunctionError::Protocol),
        }
    }

    fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
        let fields = self.ask(&format!("version\t{}", encode(distribution)), &[])?;
        match (
            fields.first().map(String::as_str),
            fields.get(1).map(String::as_str),
        ) {
            (Some("missing"), _) => Ok(InstalledVersion::Missing),
            (Some("unreadable"), _) => Ok(InstalledVersion::Unreadable),
            (Some("version"), Some(text)) => decode(text).map(InstalledVersion::Version),
            _ => Err(FunctionError::Protocol),
        }
    }

    fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
        self.ask("environment", &[])?
            .iter()
            .map(|field| {
                let (key, value) = field
                    .split_once('=')
                    .ok_or_else(|| worker_failure(format!("a marker names no key: {field}")))?;
                Ok((decode(key)?, decode(value)?))
            })
            .collect()
    }

    fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
        let fields = self.ask("integer_digits", &[])?;
        let digits: usize = fields
            .first()
            .and_then(|text| text.parse().ok())
            .ok_or(FunctionError::Protocol)?;
        Ok((digits > 0).then_some(digits))
    }
}

impl ProjectFunctions for Worker {
    fn verify_lock(
        &self,
        lock: &LockContext<'_>,
        functions: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        if lock.language != self.language {
            return Err(FunctionError::Protocol);
        }
        match (self.language, lock.kind) {
            (HostLanguage::Python, LockKind::Uv) => {
                yamaa::host::functions::verify_uv_lock(lock.bytes, functions, self)?;
            }
            (HostLanguage::R, LockKind::Renv) => {
                let locked = yamaa::host::functions::decode_renv_lock(lock.bytes)
                    .map_err(FunctionError::LockSyntax)?;
                let mut request = format!("verify\t{}", functions.len());
                for function in functions {
                    request.push('\t');
                    request.push_str(&encode(&function.call));
                }
                for (package, version) in &locked.packages {
                    let _ = write!(request, "\t{}={}", encode(package), encode(version));
                }
                let findings = self
                    .ask(&request, &[])?
                    .iter()
                    .map(|field| {
                        let parts: Vec<&str> = field.split('|').collect();
                        let [package, reason, expected, actual] = parts.as_slice() else {
                            return Err(worker_failure(format!(
                                "a lock finding has no four parts: {field}"
                            )));
                        };
                        let reason = LockReason::parse(&decode(reason)?).ok_or_else(|| {
                            worker_failure(format!("a lock finding names no reason: {field}"))
                        })?;
                        let optional = |text: &str| -> Result<Option<String>, FunctionError> {
                            Ok(if *text == *"-" {
                                None
                            } else {
                                Some(decode(text)?)
                            })
                        };
                        Ok(LockFinding::new(
                            decode(package)?,
                            reason,
                            optional(expected)?.into_iter().collect(),
                            optional(actual)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !findings.is_empty() {
                    return Err(FunctionError::Lock(findings));
                }
            }
            _ => return Err(FunctionError::Protocol),
        }
        self.same_engine()
    }

    fn bind(
        &self,
        identity: &FunctionIdentity,
        signature: &Signature,
    ) -> Result<FunctionHandle, FunctionError> {
        let mut request = format!(
            "bind\t{}\t{}",
            encode(&identity.call),
            column_type(signature.returns)
        );
        for parameter in &signature.parameters {
            request.push('\t');
            request.push_str(&encode(&parameter.name));
        }
        let fields = self.ask(&request, &[])?;
        let handle: u64 = fields
            .first()
            .and_then(|text| text.parse().ok())
            .ok_or(FunctionError::Protocol)?;
        Ok(FunctionHandle::new(handle))
    }

    fn invoke(
        &self,
        handle: &FunctionHandle,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, CallError> {
        let handle = handle
            .with::<u64, _>(|handle| *handle)
            .ok_or(CallError::Raised(FunctionError::Protocol))?;
        let mut request = format!("invoke\t{handle}");
        for argument in arguments {
            let _ = write!(
                request,
                "\t{}={}",
                encode(argument.name),
                encode_value(argument.value)
            );
        }
        let fields = self.ask(&request, &[]).map_err(raised)?;
        decode_value(fields.first().map_or("", String::as_str)).map_err(CallError::InvalidResult)
    }

    /// The worker calls the function once per row, so a batch is the row by
    /// row reference with one exchange.
    fn call_batch(
        &self,
        handle: &FunctionHandle,
        rows: usize,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Scalar>, CallError> {
        let handle = handle
            .with::<u64, _>(|handle| *handle)
            .ok_or(CallError::Raised(FunctionError::Protocol))?;
        let request = format!("batch\t{handle}\t{rows}\t{}", arguments.len());
        let lines = arguments
            .iter()
            .map(|argument| {
                let mut line = encode(&argument.name);
                for value in &argument.values {
                    line.push('\t');
                    line.push_str(&encode_value(ScalarRef::from_owned(value)));
                }
                line
            })
            .collect::<Vec<_>>();
        let fields = self.ask(&request, &lines).map_err(raised)?;
        if fields.len() != rows {
            return Err(CallError::InvalidResult(FunctionError::Rejected {
                reason: format!(
                    "the worker answered {} results for {rows} rows",
                    fields.len()
                ),
                returned: None,
            }));
        }
        fields
            .iter()
            .map(|field| decode_value(field).map_err(CallError::InvalidResult))
            .collect()
    }
}

/// A failure of a call: a rejected result the worker reported, or anything
/// else, which the call raised.
fn raised(error: FunctionError) -> CallError {
    match error {
        rejected @ FunctionError::Rejected { .. } => CallError::InvalidResult(rejected),
        other => CallError::Raised(other),
    }
}

/// The declared result type, as the function file spells it.
fn column_type(returns: yamaa::host::functions::ColumnType) -> &'static str {
    use yamaa::host::functions::ColumnType;
    match returns {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
        _ => "str",
    }
}

/// Percent-encode `text`: a byte of `A-Za-z0-9-._~` as itself, any other as
/// `%` and two uppercase hexadecimal digits.
pub(crate) fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Decode what [`encode`] made, refusing anything else as a protocol
/// failure.
pub(crate) fn decode(text: &str) -> Result<String, FunctionError> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            let digits = bytes
                .get(index + 1..index + 3)
                .and_then(|digits| std::str::from_utf8(digits).ok())
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                .ok_or(FunctionError::Protocol)?;
            decoded.push(digits);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| FunctionError::Protocol)
}

/// One value as the protocol spells it.
pub(crate) fn encode_value(value: ScalarRef<'_>) -> String {
    match value {
        ScalarRef::Missing => "-".to_owned(),
        ScalarRef::Str(text) => format!("s:{}", encode(text)),
        ScalarRef::Int(value) => format!("i:{value}"),
        ScalarRef::Float(value) => format!("f:{}", float_text(value)),
        ScalarRef::Bool(value) => format!("b:{value}"),
        ScalarRef::Date(date) => {
            let (year, month, day) = date.fields();
            format!("d:{year:04}-{month:02}-{day:02}")
        }
        ScalarRef::DateTime(datetime) => {
            let (year, month, day, hour, minute, second) = datetime.fields();
            format!("t:{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
        }
        // The facade enum is non-exhaustive; a new variant must not reach
        // the worker as some other value.
        _ => "-".to_owned(),
    }
}

/// A float as text that reads back as the same float: Rust's shortest
/// round-trip form, with `inf`, `-inf` and `nan` for the values that have no
/// decimal form.
fn float_text(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        format!("{value:?}")
    }
}

/// The value a reply spells, or why it is no value of the protocol, which
/// is a rejected result.
pub(crate) fn decode_value(field: &str) -> Result<Scalar, FunctionError> {
    let rejected = |reason: &str| FunctionError::Rejected {
        reason: format!("{reason}: {field}"),
        returned: None,
    };
    if field == "-" {
        return Ok(Scalar::Missing);
    }
    let Some((kind, text)) = field.split_once(':') else {
        return Err(rejected("a result has no type"));
    };
    match kind {
        "s" => decode(text)
            .map(Scalar::Str)
            .map_err(|_| rejected("a str result is not percent-encoded UTF-8")),
        "i" => text
            .parse()
            .map(Scalar::Int)
            .map_err(|_| rejected("an int result is no 64-bit integer")),
        "f" => text
            .parse()
            .map(Scalar::float)
            .map_err(|_| rejected("a float result is no float")),
        "b" => match text {
            "true" => Ok(Scalar::Bool(true)),
            "false" => Ok(Scalar::Bool(false)),
            _ => Err(rejected("a bool result is neither true nor false")),
        },
        "d" => parse_date(text)
            .map(Scalar::Date)
            .ok_or_else(|| rejected("a date result names no day")),
        "t" => parse_datetime(text)
            .map(Scalar::DateTime)
            .ok_or_else(|| rejected("a datetime result names no second")),
        _ => Err(rejected("a result has a type the protocol does not spell")),
    }
}

fn parse_date(text: &str) -> Option<Date> {
    let mut parts = text.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    Date::new(year, month, day)
}

fn parse_datetime(text: &str) -> Option<DateTime> {
    let (date, time) = text.split_once('T')?;
    let (year, month, day) = parse_date(date)?.fields();
    let mut parts = time.splitn(3, ':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    DateTime::new(year, month, day, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codec is the seam the workers' own tests cannot reach from Rust:
    /// each value spells as the protocol says and reads back as itself.
    #[test]
    fn every_value_spells_as_the_protocol_says_and_reads_back() {
        let cases = [
            (Scalar::Missing, "-"),
            (
                Scalar::Str("a b\tc/%\u{e9}".into()),
                "s:a%20b%09c%2F%25%C3%A9",
            ),
            (Scalar::Int(-42), "i:-42"),
            (Scalar::Float(0.1), "f:0.1"),
            (Scalar::Float(1e300), "f:1e300"),
            (Scalar::Bool(true), "b:true"),
            (
                Scalar::Date(Date::new(2024, 2, 29).unwrap()),
                "d:2024-02-29",
            ),
            (
                Scalar::DateTime(DateTime::new(999, 12, 31, 23, 59, 58).unwrap()),
                "t:0999-12-31T23:59:58",
            ),
        ];
        for (value, spelled) in cases {
            assert_eq!(
                encode_value(ScalarRef::from_owned(&value)),
                spelled,
                "{value:?}"
            );
            assert_eq!(decode_value(spelled).unwrap(), value, "{spelled}");
        }
        assert_eq!(encode_value(ScalarRef::Float(f64::INFINITY)), "f:inf");
        assert_eq!(encode_value(ScalarRef::Float(f64::NEG_INFINITY)), "f:-inf");
        assert_eq!(encode_value(ScalarRef::Float(f64::NAN)), "f:nan");
        // The engine turns a float that is not finite into a missing value.
        for spelled in ["f:inf", "f:-inf", "f:nan"] {
            assert_eq!(decode_value(spelled).unwrap(), Scalar::Missing, "{spelled}");
        }
    }

    #[test]
    fn a_reply_that_spells_no_value_is_a_rejected_result() {
        for (spelled, reason) in [
            ("x", "a result has no type: x"),
            ("i:1.5", "an int result is no 64-bit integer: i:1.5"),
            ("d:2024-02-30", "a date result names no day: d:2024-02-30"),
            (
                "t:2024-02-29",
                "a datetime result names no second: t:2024-02-29",
            ),
            ("b:yes", "a bool result is neither true nor false: b:yes"),
            ("s:%ZZ", "a str result is not percent-encoded UTF-8: s:%ZZ"),
            (
                "q:1",
                "a result has a type the protocol does not spell: q:1",
            ),
        ] {
            match decode_value(spelled) {
                Err(FunctionError::Rejected {
                    reason: actual,
                    returned: None,
                }) => {
                    assert_eq!(actual, reason, "{spelled}")
                }
                other => panic!("{spelled}: {other:?}"),
            }
        }
    }

    #[test]
    fn text_percent_encodes_every_byte_outside_the_unreserved_set() {
        assert_eq!(encode("yamaa_benchmarks.bmi"), "yamaa_benchmarks.bmi");
        assert_eq!(encode("base::signif"), "base%3A%3Asignif");
        assert_eq!(decode("base%3a%3Asignif").unwrap(), "base::signif");
        assert!(matches!(decode("%4"), Err(FunctionError::Protocol)));
        assert!(matches!(decode("%FF"), Err(FunctionError::Protocol)));
    }

    /// A worker that cannot start is an exception of the class `worker`
    /// naming each interpreter tried, before anything is bound.
    #[test]
    fn a_worker_that_cannot_start_names_its_interpreter() {
        let worker = Worker::new(
            HostLanguage::Python,
            Some("yamaa-no-such-interpreter".to_owned()),
        );
        match worker.engine_version() {
            Err(FunctionError::Exception { class, message, .. }) => {
                assert_eq!(class, "worker");
                assert!(
                    message
                        .starts_with("cannot start the python worker: yamaa-no-such-interpreter: "),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        let defaults = Worker::new(HostLanguage::R, None);
        assert_eq!(defaults.defaults(), ["Rscript"]);
        assert_eq!(defaults.arguments(), ["-e", "yamaa:::.yamaa_worker()"]);
    }
}
