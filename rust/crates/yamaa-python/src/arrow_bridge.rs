//! `yamaa.Table`, the table that every Python call returns (#3030 D11, D43).
//!
//! It holds an engine table handle and shares the table's immutable Arrow
//! buffers through the Arrow PyCapsule stream interface, never through IPC
//! serialization. Its counts, names and printed preview come from the
//! engine, `to_pylist()` reads a fresh stream and builds Python values here
//! without importing anything, and `to_arrow()`, `to_polars()` and
//! `to_pandas()` import their library only when called.
use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::RecordBatchReader;
use arrow_array::{
    Array, Date32Array, Float64Array, Int64Array, LargeStringArray, ListArray, StringArray,
    StringViewArray, TimestampSecondArray,
};
use arrow_schema::{DataType, TimeUnit};
use pyo3::{
    exceptions::{
        PyModuleNotFoundError, PyNotImplementedError, PyRuntimeError, PyTypeError, PyValueError,
    },
    prelude::*,
    types::{PyCapsule, PyDate, PyDateTime, PyDict, PyList, PyModule, PyString},
    IntoPyObjectExt,
};
use yamaa_boundary::client::{self, Owned, Refused};

/// A table that yamaa returned: immutable columns of str, int, float, date,
/// datetime or list values, any of which may be missing.
///
/// `len()` and `num_rows` count its rows, `column_names` names its columns,
/// and `repr()` prints a preview. `to_pylist()` gives its rows as Python
/// values and needs no other package. `to_arrow()`, `to_polars()` and
/// `to_pandas()` convert it with PyArrow, Polars or pandas, which they import
/// only when called, and any library that reads the Arrow PyCapsule stream
/// interface reads it directly.
///
/// The output of a run with `stream=True` is produced as it is read: a
/// library that reads its Arrow stream drives the run, one batch per pull,
/// and every other member reads the output whole, running the run whole if
/// nobody has started to read it. Once its stream has been read, the output
/// is not kept, and reading it again raises `RuntimeError`.
#[pyclass(frozen, module = "yamaa", name = "Table")]
pub struct Table {
    table: Owned,
}

impl Table {
    pub(crate) fn new(table: Owned) -> Self {
        Self { table }
    }

    /// A fresh stream over the table, with its own cursor.
    fn stream(&self) -> PyResult<FFI_ArrowArrayStream> {
        let stream = self.table.engine().export(self.table.handle());
        let mut stream = stream.map_err(refused)?.into_raw();
        // SAFETY: the engine's stream has the C stream interface's layout;
        // `from_raw` moves it out and leaves the source released.
        Ok(unsafe { FFI_ArrowArrayStream::from_raw((&raw mut stream).cast()) })
    }
}

/// A refusal of a table's entry, which only a boundary fault can give.
fn refused(refusal: Refused) -> PyErr {
    match refusal {
        Refused::Engine { message, .. } | Refused::Boundary(message) => {
            PyRuntimeError::new_err(message)
        }
        Refused::Cancelled => PyRuntimeError::new_err("the call was cancelled"),
    }
}

#[pymethods]
impl Table {
    /// The number of rows.
    #[getter]
    fn num_rows(&self, py: Python<'_>) -> PyResult<usize> {
        crate::interrupt::run(py, |_| {
            let rows = self.table.engine().rows(self.table.handle());
            Ok(rows.map_err(refused)? as usize)
        })
    }

    /// The column names, in order.
    #[getter]
    fn column_names(&self) -> PyResult<Vec<String>> {
        let columns = self.table.engine().columns(self.table.handle());
        columns.map_err(refused)
    }

    /// The number of rows.
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        self.num_rows(py)
    }

    /// The table's preview, the same text in every host (#3030 D167).
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        crate::interrupt::run(py, |_| {
            let preview = self.table.engine().preview(self.table.handle());
            preview.map_err(refused)
        })
    }

    /// A fresh Arrow C stream over the held buffers, as a PyCapsule named
    /// `arrow_array_stream`; for a streamed run's output, a stream that
    /// produces it as it is read. A requested schema is refused.
    #[pyo3(signature=(requested_schema=None))]
    // `PyCapsule::new` builds on the PyO3 0.27.2 floor; 0.29 renamed it
    // `new_with_value`, with the same pointer and drop behaviour.
    #[allow(deprecated)]
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        if requested_schema.is_some_and(|schema| !schema.is_none()) {
            return Err(PyNotImplementedError::new_err(
                "Arrow schema conversion is not supported; import the preserved schema first",
            ));
        }
        // PyO3 stores the stream header at the capsule pointer and drops its
        // value on capsule destruction. A consumer moves that header and clears
        // release; dropping the consumed capsule then only frees the shell.
        // Unconsumed capsules release their reader and buffers on destruction.
        PyCapsule::new(py, self.stream()?, Some(c"arrow_array_stream".into()))
    }

    /// The rows as a list of dicts, each keyed by the column names in order.
    /// A value is a str, int, float, datetime.date, naive datetime.datetime
    /// or list, and a missing value is None. Imports nothing.
    fn to_pylist<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        // Materialize first: to_pylist keeps the output available for repeated reads.
        self.num_rows(py)?;
        let reader = ArrowArrayStreamReader::try_new(self.stream()?)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let names = reader
            .schema()
            .fields()
            .iter()
            .map(|field| PyString::intern(py, field.name()))
            .collect::<Vec<_>>();
        let rows = PyList::empty(py);
        for batch in reader {
            let batch = batch.map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            let columns = batch
                .columns()
                .iter()
                .map(|array| values(py, array.as_ref()))
                .collect::<PyResult<Vec<_>>>()?;
            for row in 0..batch.num_rows() {
                let record = PyDict::new(py);
                for (name, column) in names.iter().zip(&columns) {
                    record.set_item(name, &column[row])?;
                }
                rows.append(record)?;
            }
        }
        Ok(rows)
    }

    /// The table as a `pyarrow.Table`, with the columns' Arrow types.
    fn to_arrow<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let pyarrow = import(slf.py(), "pyarrow", "to_arrow()")?;
        slf.get().num_rows(slf.py())?;
        pyarrow.call_method1("table", (slf,))
    }

    /// The table as a `polars.DataFrame`.
    fn to_polars<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let polars = import(slf.py(), "polars", "to_polars()")?;
        slf.get().num_rows(slf.py())?;
        polars.call_method1("DataFrame", (slf,))
    }

    /// The table as a `pandas.DataFrame` whose columns keep their Arrow types
    /// as `pandas.ArrowDtype`, so that every value, and every missing value,
    /// stays as it is. Needs pandas and PyArrow.
    fn to_pandas<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let pandas = import(py, "pandas", "to_pandas()")?;
        let pyarrow = import(py, "pyarrow", "to_pandas()")?;
        slf.get().num_rows(py)?;
        let options = PyDict::new(py);
        options.set_item("types_mapper", pandas.getattr("ArrowDtype")?)?;
        pyarrow
            .call_method1("table", (slf,))?
            .call_method("to_pandas", (), Some(&options))
    }
}

/// Import `package` for `method`. A package that is not installed raises a
/// `ModuleNotFoundError` naming it, chained to the import's own; an installed
/// package that fails to import raises its own error.
fn import<'py>(py: Python<'py>, package: &str, method: &str) -> PyResult<Bound<'py, PyModule>> {
    let error = match PyModule::import(py, package) {
        Ok(module) => return Ok(module),
        Err(error) => error,
    };
    let missing = error.is_instance_of::<PyModuleNotFoundError>(py)
        && error
            .value(py)
            .getattr("name")
            .and_then(|name| name.extract::<String>())
            .is_ok_and(|name| name == package);
    if !missing {
        return Err(error);
    }
    let options = PyDict::new(py);
    options.set_item("name", package)?;
    let raised = PyErr::from_value(py.get_type::<PyModuleNotFoundError>().call(
        (format!("{method} needs {package}, which is not installed"),),
        Some(&options),
    )?);
    raised.set_cause(py, Some(error));
    Err(raised)
}

/// The Python values of one Arrow column, in row order; None for a missing
/// value. A column of a type that no engine table has is refused.
fn values<'py>(py: Python<'py>, array: &dyn Array) -> PyResult<Vec<Bound<'py, PyAny>>> {
    fn cast<T: 'static>(array: &dyn Array) -> PyResult<&T> {
        array.as_any().downcast_ref::<T>().ok_or_else(|| {
            PyTypeError::new_err(format!(
                "Arrow array does not match its {} field",
                array.data_type()
            ))
        })
    }
    fn each<'py, T>(
        py: Python<'py>,
        values: impl Iterator<Item = Option<T>>,
        convert: impl Fn(T) -> PyResult<Bound<'py, PyAny>>,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        values
            .map(|value| match value {
                Some(value) => convert(value),
                None => Ok(py.None().into_bound(py)),
            })
            .collect()
    }
    let text = |value: &str| Ok(PyString::new(py, value).into_any());
    match array.data_type() {
        DataType::Utf8 => each(py, cast::<StringArray>(array)?.iter(), text),
        DataType::LargeUtf8 => each(py, cast::<LargeStringArray>(array)?.iter(), text),
        DataType::Utf8View => each(py, cast::<StringViewArray>(array)?.iter(), text),
        DataType::Int64 => each(py, cast::<Int64Array>(array)?.iter(), |value| {
            value.into_bound_py_any(py)
        }),
        DataType::Float64 => each(py, cast::<Float64Array>(array)?.iter(), |value| {
            value.into_bound_py_any(py)
        }),
        DataType::Date32 => each(py, cast::<Date32Array>(array)?.iter(), |days| {
            let (year, month, day) = client::civil_date(i64::from(days)).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "the date {days} days after 1970-01-01 is outside the years 1 to 9999"
                ))
            })?;
            Ok(PyDate::new(py, year, month, day)?.into_any())
        }),
        DataType::Timestamp(TimeUnit::Second, None) => {
            each(py, cast::<TimestampSecondArray>(array)?.iter(), |seconds| {
                let (year, month, day, hour, minute, second) = client::civil_datetime(seconds)
                    .ok_or_else(|| {
                        PyValueError::new_err(format!(
                            "the datetime {seconds} seconds after 1970-01-01T00:00:00 is \
                             outside the years 1 to 9999"
                        ))
                    })?;
                Ok(
                    PyDateTime::new(py, year, month, day, hour, minute, second, 0, None)?
                        .into_any(),
                )
            })
        }
        DataType::List(_) => {
            let lists = cast::<ListArray>(array)?;
            (0..lists.len())
                .map(|row| {
                    if lists.is_null(row) {
                        Ok(py.None().into_bound(py))
                    } else {
                        Ok(PyList::new(py, values(py, lists.value(row).as_ref())?)?.into_any())
                    }
                })
                .collect()
        }
        other => Err(PyTypeError::new_err(format!(
            "unsupported Arrow column type {other}"
        ))),
    }
}
