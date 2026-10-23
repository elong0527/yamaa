//! The separately loaded engine of the test-only loaded mode (the design on
//! #2870, sections 3.1 and 3.7; #3702). A binding built with its
//! `dynamic-engine` feature links no engine: at its handshake it loads the
//! engine library from beside its own module file and looks up the one
//! symbol every boundary version keeps, [`ENTRY`]. No shipped package
//! compiles this module; a repository check refuses the feature in every
//! build that ships.
//!
//! The library is found by an absolute path computed from the module's own
//! location, `dladdr` on Linux and macOS and `GetModuleHandleExW` with
//! `GetModuleFileNameW` on Windows, never through `PATH`, `LD_LIBRARY_PATH`,
//! `DYLD_*` or the current directory, and it is never unloaded: a library
//! with registered thread-local destructors cannot be unloaded safely.
//!
//! - Linux and macOS: `dlopen` with `RTLD_NOW | RTLD_LOCAL`, which keeps the
//!   library's symbols out of the global namespace. `dlopen` on macOS may
//!   consult `DYLD_LIBRARY_PATH` even for an absolute path, so the loader
//!   takes the entry only from the file it asked for.
//! - Windows: `LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
//!   LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`, which leaves out the current
//!   directory and `PATH`.
//!
//! # Safety
//!
//! Loading a library runs its initializers, and the loader trusts the
//! library beside the module to be an engine of this workspace, whose
//! [`ENTRY`] has the [`Negotiate`] signature: that symbol and its signature
//! are kept in every boundary version.
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::client::Negotiate;

/// The negotiation entry an engine library exports.
pub const ENTRY: &str = "yamaa_engine_negotiate";

/// The engine library's file name beside a binding's module.
#[cfg(windows)]
pub const LIBRARY: &str = "yamaa_engine.dll";
/// The engine library's file name beside a binding's module, whatever the
/// platform names a shared library: `dlopen` reads no extension.
#[cfg(not(windows))]
pub const LIBRARY: &str = "libyamaa_engine.so";

/// Why no engine was loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// The module's own file is unknown, or its path is not absolute, which
    /// would read the current directory.
    Module(String),
    /// No library loads from this path, for this reason.
    Library(PathBuf, String),
    /// The library at this path exports no [`ENTRY`].
    Entry(PathBuf),
}

/// The negotiation entry of the engine library beside the module that holds
/// this code: loaded on the first call and kept, with the library, for the
/// life of the process.
pub fn entry() -> Result<Negotiate, Missing> {
    static LOADED: OnceLock<Result<Negotiate, Missing>> = OnceLock::new();
    LOADED
        .get_or_init(|| load(&platform::module().map_err(Missing::Module)?))
        .clone()
}

/// The negotiation entry of the engine library beside `module`, a module
/// file's absolute path.
pub fn load(module: &Path) -> Result<Negotiate, Missing> {
    if !module.is_absolute() {
        return Err(Missing::Module(format!(
            "{} is not absolute",
            module.display()
        )));
    }
    let directory = module
        .parent()
        .ok_or_else(|| Missing::Module(format!("{} has no directory", module.display())))?;
    let library = directory.join(LIBRARY);
    // SAFETY: see the module documentation.
    let entry = unsafe { platform::open(&library) }?;
    // SAFETY: an engine library's entry has the negotiation entry's
    // signature, which every boundary version keeps.
    Ok(unsafe { std::mem::transmute::<*const std::ffi::c_void, Negotiate>(entry) })
}

/// An address inside the module that holds this code.
static ANCHOR: u8 = 0;

#[cfg(unix)]
mod platform {
    use std::ffi::{c_void, CStr, CString, OsStr};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use super::{Missing, ANCHOR, ENTRY};

    /// The file of the shared object that `address` lies in.
    fn file_of(address: *const c_void) -> Option<PathBuf> {
        // SAFETY: `Dl_info` is plain data, valid all zeros.
        let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
        // SAFETY: `dladdr` reads the address and writes `info`.
        if unsafe { libc::dladdr(address, &mut info) } == 0 || info.dli_fname.is_null() {
            return None;
        }
        // SAFETY: `dladdr` points `dli_fname` at a NUL-terminated path that
        // lives as long as the object.
        let name = unsafe { CStr::from_ptr(info.dli_fname) };
        Some(PathBuf::from(OsStr::from_bytes(name.to_bytes())))
    }

    pub(super) fn module() -> Result<PathBuf, String> {
        file_of(std::ptr::addr_of!(ANCHOR).cast())
            .ok_or_else(|| "dladdr finds no file for this module".to_owned())
    }

    /// The last error `dlopen` or `dlsym` reported.
    fn reported() -> String {
        // SAFETY: `dlerror` returns null or a NUL-terminated message, read
        // before the next call on this thread.
        let message = unsafe { libc::dlerror() };
        if message.is_null() {
            return "no reason given".to_owned();
        }
        // SAFETY: as above.
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }

    /// `library`'s entry, the library loaded and never closed.
    ///
    /// # Safety
    ///
    /// See the module documentation.
    pub(super) unsafe fn open(library: &Path) -> Result<*const c_void, Missing> {
        let failed = |reason: String| Missing::Library(library.to_owned(), reason);
        let path = CString::new(library.as_os_str().as_bytes())
            .map_err(|_| failed("the path holds a NUL byte".to_owned()))?;
        // SAFETY: the caller's contract; the path is NUL-terminated.
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(failed(reported()));
        }
        let name = CString::new(ENTRY).expect("the entry's name holds no NUL byte");
        // SAFETY: the handle is the library's, which is never closed.
        let entry = unsafe { libc::dlsym(handle, name.as_ptr()) };
        if entry.is_null() {
            return Err(Missing::Entry(library.to_owned()));
        }
        let canonical = |path: &Path| std::fs::canonicalize(path).ok();
        let found = file_of(entry.cast_const());
        if found.as_deref().and_then(canonical) != canonical(library) {
            let found = found.map_or("no file".to_owned(), |found| found.display().to_string());
            return Err(failed(format!("the entry was found in {found}")));
        }
        Ok(entry.cast_const())
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GetProcAddress, LoadLibraryExW,
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };

    use super::{Missing, ANCHOR, ENTRY};

    /// The file a loaded module was loaded from.
    fn file_of(module: HMODULE) -> Option<PathBuf> {
        let mut buffer = vec![0u16; 512];
        loop {
            let capacity = u32::try_from(buffer.len()).ok()?;
            // SAFETY: the buffer holds `capacity` writable units.
            let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), capacity) };
            let length = usize::try_from(length).ok()?;
            if length == 0 {
                return None;
            }
            if length < buffer.len() {
                buffer.truncate(length);
                return Some(PathBuf::from(OsString::from_wide(&buffer)));
            }
            // A truncated path; Windows paths end within 32,767 units.
            if buffer.len() > 32_767 {
                return None;
            }
            buffer.resize(buffer.len() * 2, 0);
        }
    }

    pub(super) fn module() -> Result<PathBuf, String> {
        let mut module: HMODULE = std::ptr::null_mut();
        let flags =
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        // SAFETY: the address lies in this module, and the call writes the
        // module's handle without taking a reference.
        let found =
            unsafe { GetModuleHandleExW(flags, std::ptr::addr_of!(ANCHOR).cast(), &mut module) };
        if found == 0 {
            return Err("GetModuleHandleExW finds no module for this code".to_owned());
        }
        file_of(module).ok_or_else(|| "GetModuleFileNameW gives no file".to_owned())
    }

    /// `library`'s entry, the library loaded and never freed.
    ///
    /// # Safety
    ///
    /// See the module documentation.
    pub(super) unsafe fn open(library: &Path) -> Result<*const c_void, Missing> {
        let failed = |reason: String| Missing::Library(library.to_owned(), reason);
        let path: Vec<u16> = library.as_os_str().encode_wide().chain([0]).collect();
        let flags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
        // SAFETY: the caller's contract; the path is NUL-terminated.
        let module = unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), flags) };
        if module.is_null() {
            let error = std::io::Error::last_os_error();
            return Err(failed(error.to_string()));
        }
        let canonical = |path: &Path| std::fs::canonicalize(path).ok();
        if file_of(module).as_deref().and_then(canonical) != canonical(library) {
            return Err(failed("another module of that name was loaded".to_owned()));
        }
        let name: Vec<u8> = ENTRY.bytes().chain([0]).collect();
        // SAFETY: the module is the library's, which is never freed.
        match unsafe { GetProcAddress(module, name.as_ptr()) } {
            Some(entry) => Ok(entry as *const c_void),
            None => Err(Missing::Entry(library.to_owned())),
        }
    }
}
