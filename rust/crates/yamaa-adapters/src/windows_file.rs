//! Owned, descriptor-relative Windows file access. Study paths remain a caller concern.
#![deny(unsafe_op_in_unsafe_fn)]
use std::{
    fs::{File, OpenOptions},
    io,
    mem::size_of,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            NtCreateFile, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        },
    },
    Win32::{
        Foundation::{RtlNtStatusToDosError, HANDLE, OBJ_CASE_INSENSITIVE, UNICODE_STRING},
        Storage::FileSystem::{
            FileAttributeTagInfo, FileIdInfo, GetFileInformationByHandleEx, GetFileType,
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
            FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_TYPE_DISK, SYNCHRONIZE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub volume: u64,
    pub file: [u8; 16],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Directory,
    RegularFile,
    Symlink,
    Other,
}
#[derive(Debug)]
pub struct Status {
    pub identity: Identity,
    pub kind: Kind,
}
#[derive(Debug)]
pub struct Directory(File);

impl Directory {
    /// Caller-selected absolute root; descendants are opened only by this held handle.
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "root must be absolute",
            ));
        }
        // FILE_ID_INFO identifies local volume/file pairs on this computer.
        // Remote/device namespaces need separate qualification and identity scoping.
        let Some(std::path::Component::Prefix(prefix)) = path.components().next() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "root must name a local drive",
            ));
        };
        if !matches!(
            prefix.kind(),
            std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "remote or device roots are not qualified",
            ));
        }
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        Self::from_file(file)
    }
    fn from_file(file: File) -> io::Result<Self> {
        require_kind(&file, Kind::Directory)?;
        Ok(Self(file))
    }
    pub fn identity(&self) -> io::Result<Identity> {
        Ok(status(&self.0)?.identity)
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }
    /// Exactly one child component. No full path reconstruction or link traversal.
    pub fn directory(&self, name: &str) -> io::Result<Self> {
        Self::from_file(self.open_child(name, FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE)?)
    }
    /// Metadata only. Reparse points remain visible and grant no traversal authority.
    pub fn inspect(&self, name: &str) -> io::Result<Status> {
        status(&self.open_child(name, FILE_READ_ATTRIBUTES | SYNCHRONIZE)?)
    }
    pub fn read_file(&self, name: &str) -> io::Result<File> {
        let file = self.open_child(name, FILE_READ_ATTRIBUTES | FILE_READ_DATA | SYNCHRONIZE)?;
        require_kind(&file, Kind::RegularFile)?;
        Ok(file)
    }
    fn open_child(&self, name: &str, access: u32) -> io::Result<File> {
        self.child(
            name,
            access,
            FILE_OPEN,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null_mut(),
        )
    }
    fn child(
        &self,
        name: &str,
        access: u32,
        disposition: u32,
        options: u32,
        security: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
    ) -> io::Result<File> {
        let mut name = child_name(name)?;
        let bytes = u16::try_from(name.len() * 2)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "child name too long"))?;
        let mut unicode = UNICODE_STRING {
            Length: bytes,
            MaximumLength: bytes,
            Buffer: name.as_mut_ptr(),
        };
        let attrs = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: self.0.as_raw_handle(),
            ObjectName: &mut unicode,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: security.cast(),
            SecurityQualityOfService: ptr::null_mut(),
        };
        let mut result: HANDLE = ptr::null_mut();
        let mut iosb = IO_STATUS_BLOCK::default();
        // SAFETY: both output slots and all referenced buffers live through this synchronous
        // call. The root File owns a valid handle. The counted UTF-16 name has no separators,
        // NUL, ADS syntax or traversal components. No kernel-only attributes are used.
        let code = unsafe {
            NtCreateFile(
                &mut result,
                access,
                &attrs,
                &mut iosb,
                ptr::null(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                disposition,
                options,
                ptr::null(),
                0,
            )
        };
        if code < 0 {
            // SAFETY: this routine maps a returned status value and borrows no buffers.
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(code) } as i32,
            ));
        }
        if result.is_null() {
            return Err(io::Error::other("successful file open returned no handle"));
        }
        // SAFETY: successful synchronous NtCreateFile transferred one newly owned handle.
        Ok(unsafe { File::from_raw_handle(result) })
    }
}
fn child_name(name: &str) -> io::Result<Vec<u16>> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\', ':', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid relative child name",
        ));
    }
    let name: Vec<u16> = name.encode_utf16().collect();
    if name.len() > u16::MAX as usize / 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child name too long",
        ));
    }
    Ok(name)
}
/// Published names must remain usable by ordinary Win32 file tools.
pub(crate) fn validate_publication_name(name: &str) -> io::Result<()> {
    child_name(name)?;
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    let device = ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        || (stem.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
        }) && matches!(
            stem.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        ));
    if device
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c <= '\u{1f}' || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication name is not representable through Win32 paths",
        ));
    }
    Ok(())
}
pub fn status(file: &File) -> io::Result<Status> {
    let mut attrs = FILE_ATTRIBUTE_TAG_INFO::default();
    let mut id = FILE_ID_INFO::default();
    // SAFETY: each output is an initialized SDK-layout struct with exactly its buffer size;
    // File keeps its handle alive and the documented information class matches the struct.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut attrs as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: same lifetime/layout guarantees, using FILE_ID_INFO and FileIdInfo.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: File owns this valid, live handle; GetFileType retains no reference.
    let disk = unsafe { GetFileType(file.as_raw_handle()) } == FILE_TYPE_DISK;
    let kind = if attrs.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        Kind::Symlink
    } else if !disk {
        Kind::Other
    } else if attrs.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        Kind::Directory
    } else {
        Kind::RegularFile
    };
    Ok(Status {
        identity: Identity {
            volume: id.VolumeSerialNumber,
            file: id.FileId.Identifier,
        },
        kind,
    })
}
fn require_kind(file: &File, required: Kind) -> io::Result<()> {
    match status(file)?.kind {
        kind if kind == required => Ok(()),
        Kind::Symlink => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "resource path contains a reparse point",
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "resource path has the wrong file kind",
        )),
    }
}

use std::os::windows::io::OwnedHandle;
use windows_sys::{
    Wdk::Storage::FileSystem::{
        FileRenameInformation, NtSetInformationFile, FILE_CREATE, FILE_DIRECTORY_FILE,
        FILE_NON_DIRECTORY_FILE, FILE_RENAME_INFORMATION,
    },
    Win32::{
        Foundation::{
            LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_TOKEN, WAIT_ABANDONED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
        },
        Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO,
            FILE_WRITE_DATA,
        },
        System::Threading::{
            CreateMutexW, GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
            ReleaseMutex, WaitForSingleObject,
        },
    },
};

struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: the SDK allocated this buffer with LocalAlloc; this owner frees it once.
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// Use the effective user, rather than the token's possibly group-valued default owner.
/// An impersonating thread never falls back after an access/anonymous-token failure.
fn effective_user_sid() -> io::Result<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: pseudo-thread handle is valid and the writable output lives through the call.
    let ok = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut raw) };
    if ok == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_TOKEN as i32) {
            return Err(error);
        }
        // SAFETY: with no thread token, query the process token; no privileges are changed.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // SAFETY: successful Open*Token transferred one owned, non-null token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut bytes = 0;
    // SAFETY: NULL/zero queries only the required size into this live output slot.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut bytes,
        )
    };
    let error = io::Error::last_os_error();
    if ok != 0 || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error);
    }
    if bytes < size_of::<TOKEN_USER>() as u32 || bytes > 65_536 {
        return Err(io::Error::other("invalid token user buffer size"));
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    let capacity = bytes;
    // SAFETY: usize storage has TOKEN_USER alignment and at least capacity writable bytes.
    // The token and resulting SID-containing buffer remain alive through SID conversion.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            capacity,
            &mut bytes,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if bytes < size_of::<TOKEN_USER>() as u32 || bytes > capacity {
        return Err(io::Error::other("invalid returned token user size"));
    }
    // SAFETY: successful TokenUser query initialized this aligned TOKEN_USER header.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = ptr::null_mut();
    // SAFETY: the SDK-produced SID points into the still-held TokenUser buffer.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(text.cast());
    let mut length = 0;
    // SAFETY: successful conversion guarantees an allocated NUL-terminated UTF-16 SID.
    // Its standard numeric spelling is bounded by Windows' maximum SID subauthority count.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: these length UTF-16 units precede the SDK-guaranteed terminator.
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(|_| io::Error::other("invalid token user SID spelling"))
}
struct PrivateSecurity(LocalAllocation);
impl PrivateSecurity {
    fn new() -> io::Result<Self> {
        let sid = effective_user_sid()?;
        // Protected, inheritable full access for the effective user only. Set the owner
        // explicitly too; an administrator-group default owner must not exclude this user.
        let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        // SAFETY: terminated SDK-formatted SID/SDDL and writable output live through the call.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(LocalAllocation(descriptor)))
    }
}
impl Directory {
    pub fn create_private_directory(&self, name: &str) -> io::Result<Self> {
        let security = PrivateSecurity::new()?;
        let file = self.child(
            name,
            FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE | DELETE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            security.0 .0,
        )?;
        // FILE_CREATE plus FILE_DIRECTORY_FILE grants one newly created directory
        // handle. Return that ownership before fallible metadata checks so callers
        // can clean it by handle and report any cleanup failure.
        Ok(Self(file))
    }
    pub fn create_file(&self, name: &str) -> io::Result<File> {
        let file = self.child(
            name,
            FILE_READ_ATTRIBUTES | FILE_WRITE_DATA | SYNCHRONIZE | DELETE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null_mut(),
        )?;
        require_kind(&file, Kind::RegularFile)?;
        Ok(file)
    }
    pub fn remove(&self) -> io::Result<()> {
        mark_delete(&self.0)
    }
    /// Rename the held source to one validated child of this held directory.
    pub fn replace(&self, source: &File, name: &str) -> io::Result<()> {
        validate_publication_name(name)?;
        let name = child_name(name)?;
        let bytes = size_of::<FILE_RENAME_INFORMATION>() + name.len() * size_of::<u16>();
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        let mut iosb = IO_STATUS_BLOCK::default();
        // SAFETY: usize storage supplies SDK alignment and the entire header plus counted
        // UTF-16 tail, initialized to zero. The synchronous source handle owns DELETE access.
        // The held destination directory and all buffers outlive the call; one child only.
        let code = unsafe {
            (*info).Anonymous.ReplaceIfExists = true;
            (*info).RootDirectory = self.0.as_raw_handle();
            (*info).FileNameLength = (name.len() * 2) as u32;
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                ptr::addr_of_mut!((*info).FileName).cast(),
                name.len(),
            );
            NtSetInformationFile(
                source.as_raw_handle(),
                &mut iosb,
                info.cast(),
                bytes as u32,
                FileRenameInformation,
            )
        };
        if code < 0 {
            // SAFETY: status conversion borrows no buffers and changes no state.
            Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(code) } as i32,
            ))
        } else {
            Ok(())
        }
    }
}
pub fn mark_delete(file: &File) -> io::Result<()> {
    let mut info = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: matching initialized SDK layout and exact buffer size, with a live owned handle.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&mut info as *mut FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub struct DirectoryLock {
    handle: OwnedHandle,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl DirectoryLock {
    pub fn acquire(directory: &Directory) -> io::Result<Self> {
        let id = directory.identity()?;
        // OS physical identity, rendered directly; no content/provenance digest.
        let name = format!(
            "Global\\yamaa-publication-{}-{}",
            id.volume,
            u128::from_le_bytes(id.file)
        );
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: terminated name lives through the call; NULL security makes a non-inherited
        // handle with token-default ACL. Named mutex authority grants no filesystem access.
        let raw = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateMutexW returned one newly owned valid handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: handle stays live. Zero timeout preserves non-blocking publication.
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self {
                handle,
                _thread: std::marker::PhantomData,
            }),
            WAIT_TIMEOUT => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "publication directory is locked",
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        // SAFETY: acquire obtained mutex ownership on this same thread; the !Send marker
        // prevents transferring the guard. OwnedHandle subsequently closes it exactly once.
        unsafe {
            ReleaseMutex(self.handle.as_raw_handle());
        }
    }
}

/// Canonical spelling of one physical entry, never a source of path authority.
pub fn file_name(file: &File) -> io::Result<String> {
    use windows_sys::Win32::Storage::FileSystem::{FileNameInfo, FILE_NAME_INFO};
    const MAX_BYTES: usize = 65_536;
    let offset = std::mem::offset_of!(FILE_NAME_INFO, FileName);
    let mut buffer = vec![0usize; (MAX_BYTES + offset).div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_NAME_INFO>();
    // SAFETY: aligned initialized storage covers the fixed header and bounded UTF-16 tail;
    // the live File owns its handle and the information class matches FILE_NAME_INFO.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileNameInfo,
            info.cast(),
            (MAX_BYTES + offset) as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: success initialized the header; the returned tail length is checked before read.
    let bytes = unsafe { (*info).FileNameLength as usize };
    if bytes > MAX_BYTES || bytes % 2 != 0 {
        return Err(io::Error::other("invalid returned file name length"));
    }
    // SAFETY: the checked tail lies entirely within the owned aligned output allocation.
    let text = unsafe {
        std::slice::from_raw_parts(ptr::addr_of!((*info).FileName).cast::<u16>(), bytes / 2)
    };
    let text = String::from_utf16(text)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-Unicode file name"))?;
    let name = text
        .rsplit('\\')
        .next()
        .ok_or_else(|| io::Error::other("missing file name"))?;
    child_name(name)?;
    Ok(name.into())
}
impl Directory {
    pub fn name(&self) -> io::Result<String> {
        file_name(&self.0)
    }
}
pub fn equal_name(left: &str, right: &str) -> bool {
    use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
    let left: Vec<u16> = left.encode_utf16().collect();
    let right: Vec<u16> = right.encode_utf16().collect();
    let (Ok(left_length), Ok(right_length)) =
        (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return false;
    };
    // SAFETY: counted initialized UTF-16 buffers live through a pure ordinal comparison.
    unsafe {
        CompareStringOrdinal(left.as_ptr(), left_length, right.as_ptr(), right_length, 1)
            == CSTR_EQUAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Read,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Sandbox(std::path::PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-win-handles-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
        fn root(&self) -> Directory {
            Directory::open(&self.0).unwrap()
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn remote_device_and_relative_roots_are_refused_before_native_open() {
        for path in [
            "relative",
            "//server/share",
            "//?/UNC/server/share",
            "//./pipe/name",
        ] {
            assert_eq!(
                Directory::open(Path::new(path)).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
    #[test]
    fn child_names_refuse_escape_ads_and_unbounded_unicode() {
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        for name in [
            "",
            ".",
            "..",
            "../outside",
            "child/next",
            "child\\next",
            "C:relative",
            "data.csv:secret",
            "nul\0tail",
        ] {
            assert_eq!(
                root.inspect(name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{name:?}"
            );
        }
        assert_eq!(
            root.inspect(&"x".repeat(32768)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn refused_publication_names_leave_the_owned_candidate_and_target_unchanged() {
        use std::io::Write;
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        std::fs::write(sandbox.0.join("output.csv"), b"old").unwrap();
        let mut candidate = root.create_file("candidate.part").unwrap();
        candidate.write_all(b"new").unwrap();
        for name in [
            "out.csv.",
            "out.csv ",
            "CON",
            "con.csv",
            "PRN",
            "aux.txt",
            "NUL.tar.gz",
            "COM1",
            "com9.csv",
            "LPT1",
            "lpt9.csv",
            "COM¹.csv",
            "lpt²",
            "COM³",
            "NUL .csv",
            "bad<char",
            "bad>char",
            "bad\"char",
            "bad|char",
            "bad?char",
            "bad*char",
            "bad\u{1f}char",
            "../escape",
            "output.csv:stream",
        ] {
            assert_eq!(
                root.replace(&candidate, name).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{name:?}"
            );
            assert_eq!(
                root.inspect("candidate.part").unwrap().identity,
                status(&candidate).unwrap().identity
            );
            assert_eq!(std::fs::read(sandbox.0.join("output.csv")).unwrap(), b"old");
            assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 2);
        }
        for name in [
            "COM0.csv",
            "COM10.csv",
            "LPT0.csv",
            "LPT10.csv",
            "console.csv",
            ".hidden",
            "mu-μ.csv",
        ] {
            validate_publication_name(name).unwrap();
        }
        root.replace(&candidate, "mu-μ.csv").unwrap();
        drop(candidate);
        assert_eq!(std::fs::read(sandbox.0.join("mu-μ.csv")).unwrap(), b"new");
        assert!(!sandbox.0.join("candidate.part").exists());
    }
    #[test]
    fn owned_handles_read_exact_bytes_distinguish_kinds_and_preserve_missing() {
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        std::fs::create_dir(sandbox.0.join("nested")).unwrap();
        std::fs::write(
            sandbox.0.join("nested").join("unicode-\u{03bc}.csv"),
            b"held\0bytes\xff",
        )
        .unwrap();
        let nested = root.directory("nested").unwrap();
        assert_eq!(root.inspect("nested").unwrap().kind, Kind::Directory);
        assert!(root.read_file("nested").is_err());
        assert!(nested.directory("unicode-\u{03bc}.csv").is_err());
        let mut file = nested.read_file("unicode-\u{03bc}.csv").unwrap();
        let mut actual = Vec::new();
        file.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, b"held\0bytes\xff");
        assert_eq!(
            nested.inspect("missing.csv").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            nested.inspect("unicode-\u{03bc}.csv").unwrap().identity,
            status(&file).unwrap().identity
        );
    }
    #[test]
    fn private_staging_grants_the_effective_user_reopened_access() {
        let sandbox = Sandbox::new();
        let directory = sandbox.root();
        let private = directory.create_private_directory("private-stage").unwrap();
        let file = private.create_file("candidate.part").unwrap();
        let reopened = directory.directory("private-stage").unwrap();
        assert_eq!(reopened.identity().unwrap(), private.identity().unwrap());
        assert_eq!(
            status(&reopened.read_file("candidate.part").unwrap())
                .unwrap()
                .identity,
            status(&file).unwrap().identity
        );
        drop(file);
        drop(reopened);
        drop(private);
    }
    #[test]
    fn publication_keeps_private_permissions_instead_of_inheriting_other_users() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSidToSidW,
                    GetSecurityInfo, SE_FILE_OBJECT,
                },
                EqualSid, GetAce, SetFileSecurityW, ACCESS_ALLOWED_ACE, ACE_HEADER,
                DACL_SECURITY_INFORMATION,
            },
            Storage::FileSystem::{FILE_ALL_ACCESS, READ_CONTROL},
        };
        fn dacl(path: &Path) -> String {
            let file = OpenOptions::new()
                .access_mode(READ_CONTROL)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .unwrap();
            let mut descriptor = ptr::null_mut();
            // SAFETY: the owned fixture handle grants READ_CONTROL; output slots stay live.
            let code = unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut descriptor,
                )
            };
            assert_eq!(code, 0);
            let _descriptor = LocalAllocation(descriptor);
            let mut text = ptr::null_mut();
            // SAFETY: SDK-produced descriptor stays owned until the converted text is copied.
            assert_ne!(
                unsafe {
                    ConvertSecurityDescriptorToStringSecurityDescriptorW(
                        descriptor,
                        SDDL_REVISION_1,
                        DACL_SECURITY_INFORMATION,
                        &mut text,
                        ptr::null_mut(),
                    )
                },
                0
            );
            let _text = LocalAllocation(text.cast());
            let mut length = 0;
            // SAFETY: conversion guarantees NUL-terminated UTF-16 text in its allocation.
            while unsafe { *text.add(length) } != 0 {
                length += 1;
            }
            // SAFETY: length addresses only the initialized text preceding its terminator.
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap()
        }
        fn assert_only_user(path: &Path, sid: &str) {
            let file = OpenOptions::new()
                .access_mode(READ_CONTROL)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .open(path)
                .unwrap();
            let mut descriptor = ptr::null_mut();
            let mut acl = ptr::null_mut();
            // SAFETY: the live file grants READ_CONTROL and writable outputs stay live.
            assert_eq!(
                unsafe {
                    GetSecurityInfo(
                        file.as_raw_handle(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        &mut acl,
                        ptr::null_mut(),
                        &mut descriptor,
                    )
                },
                0
            );
            let _descriptor = LocalAllocation(descriptor);
            assert!(!acl.is_null(), "a NULL DACL would allow every user");
            // SAFETY: the SDK returned a valid ACL within the still-owned descriptor.
            assert_eq!(unsafe { (*acl).AceCount }, 1, "exactly one user grant");
            let mut ace = ptr::null_mut();
            // SAFETY: the valid ACL has one entry; GetAce returns its held descriptor address.
            assert_ne!(unsafe { GetAce(acl, 0, &mut ace) }, 0);
            assert!(!ace.is_null());
            // SAFETY: GetAce returned a valid initialized ACE_HEADER.
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            assert_eq!(header.AceType, 0, "ACCESS_ALLOWED_ACE_TYPE");
            assert!(header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>());
            // SAFETY: the type and full fixed header size were checked before this cast.
            let grant = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            assert_eq!(grant.Mask, FILE_ALL_ACCESS);
            let text: Vec<u16> = sid.encode_utf16().chain(Some(0)).collect();
            let mut expected = ptr::null_mut();
            // SAFETY: the terminated numeric user SID is valid, and output stays live.
            assert_ne!(
                unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut expected) },
                0
            );
            let _expected = LocalAllocation(expected);
            // SAFETY: the valid allowed ACE's SID tail and parsed expected SID stay owned.
            assert_ne!(
                unsafe { EqualSid(ptr::addr_of!(grant.SidStart).cast_mut().cast(), expected) },
                0
            );
        }
        let sandbox = Sandbox::new();
        let sid = effective_user_sid().unwrap();
        let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FR;;;BU)")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        // SAFETY: terminated fixture SDDL and writable output live through the SDK call.
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            },
            0
        );
        let _descriptor = LocalAllocation(descriptor);
        let path: Vec<u16> = sandbox.0.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: fixture-owned path is terminated and descriptor remains live.
        assert_ne!(
            unsafe { SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) },
            0
        );
        assert!(dacl(&sandbox.0).contains(";;;BU)"));
        let target = sandbox.0.join("output.csv");
        std::fs::write(&target, b"previous").unwrap();
        assert!(dacl(&target).contains(";;;BU)"));
        let mut publisher =
            crate::file_publication::Publisher::new("output.csv", target.to_str().unwrap())
                .unwrap();
        publisher.publish("output.csv", b"checked").unwrap();
        let published = dacl(&target);
        assert!(!published.contains(";;;BU)"), "{published}");
        // SDDL may render a well-known user with an alias such as LA. Compare identities.
        assert_only_user(&target, &sid);
        assert_eq!(std::fs::read(target).unwrap(), b"checked");
    }
    #[test]
    fn physical_identity_matches_hardlinks_and_changes_on_replacement() {
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        std::fs::write(sandbox.0.join("first.csv"), b"old").unwrap();
        std::fs::hard_link(sandbox.0.join("first.csv"), sandbox.0.join("alias.csv")).unwrap();
        let original = root.read_file("first.csv").unwrap();
        assert_eq!(
            status(&original).unwrap().identity,
            root.inspect("alias.csv").unwrap().identity
        );
        std::fs::rename(sandbox.0.join("first.csv"), sandbox.0.join("moved.csv")).unwrap();
        std::fs::write(sandbox.0.join("first.csv"), b"replacement").unwrap();
        assert_ne!(
            status(&original).unwrap().identity,
            root.inspect("first.csv").unwrap().identity
        );
        assert_eq!(
            status(&original).unwrap().identity,
            root.inspect("moved.csv").unwrap().identity
        );
    }
    #[test]
    fn retained_directory_handles_cannot_be_redirected_by_root_name_replacement() {
        let sandbox = Sandbox::new();
        let selected = sandbox.0.join("selected");
        let moved = sandbox.0.join("moved");
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("data.csv"), b"selected").unwrap();
        let root = Directory::open(&selected).unwrap();
        let identity = root.identity().unwrap();
        std::fs::rename(&selected, &moved).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("data.csv"), b"replacement").unwrap();
        let mut actual = Vec::new();
        root.read_file("data.csv")
            .unwrap()
            .read_to_end(&mut actual)
            .unwrap();
        assert_eq!(actual, b"selected");
        assert_eq!(root.identity().unwrap(), identity);
        assert_ne!(
            Directory::open(&selected).unwrap().identity().unwrap(),
            identity
        );
    }

    #[test]
    fn canonical_child_spelling_and_ordinal_root_comparison_preserve_case_aliases() {
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        std::fs::write(sandbox.0.join("MiXeD.csv"), b"selected").unwrap();
        let file = root.read_file("mixed.CSV").unwrap();
        assert_eq!(file_name(&file).unwrap(), "MiXeD.csv");
        assert!(equal_name("C:/Study", "c:/study"));
        assert!(!equal_name("Study", "Other"));
    }
    #[test]
    fn symbolic_files_and_directories_are_visible_but_grant_no_read_or_traversal() {
        let sandbox = Sandbox::new();
        let outside = Sandbox::new();
        let root = sandbox.root();
        std::fs::write(outside.0.join("secret.csv"), b"external").unwrap();
        std::os::windows::fs::symlink_file(
            outside.0.join("secret.csv"),
            sandbox.0.join("link.csv"),
        )
        .unwrap();
        std::os::windows::fs::symlink_dir(&outside.0, sandbox.0.join("linked")).unwrap();
        assert_eq!(root.inspect("link.csv").unwrap().kind, Kind::Symlink);
        assert_eq!(root.inspect("linked").unwrap().kind, Kind::Symlink);
        assert_eq!(
            root.read_file("link.csv").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            root.directory("linked").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(Directory::open(&sandbox.0.join("linked")).is_err());
        assert_eq!(
            std::fs::read(outside.0.join("secret.csv")).unwrap(),
            b"external"
        );
    }
    #[test]
    fn cooperating_threads_share_a_nonblocking_lock_for_physical_directory_identity() {
        let sandbox = Sandbox::new();
        let root = sandbox.root();
        let lock = DirectoryLock::acquire(&root).unwrap();
        let other = root.try_clone().unwrap();
        let observed = std::thread::spawn(move || match DirectoryLock::acquire(&other) {
            Err(error) => error.kind(),
            Ok(_) => panic!("concurrent publication acquired the same physical-directory lock"),
        })
        .join()
        .unwrap();
        assert_eq!(observed, io::ErrorKind::WouldBlock);
        drop(lock);
        assert!(DirectoryLock::acquire(&root).is_ok());
    }
}
