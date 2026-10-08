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
    Wdk::Storage::FileSystem::{FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE},
    Win32::{
        Foundation::{LocalFree, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR,
        },
        Storage::FileSystem::{
            FileDispositionInfo, FileRenameInfo, SetFileInformationByHandle, DELETE,
            FILE_DISPOSITION_INFO, FILE_RENAME_INFO, FILE_WRITE_DATA,
        },
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
    },
};

struct PrivateSecurity(PSECURITY_DESCRIPTOR);
impl PrivateSecurity {
    fn new() -> io::Result<Self> {
        // Protected owner-rights DACL, inherited by stage children.
        let sddl: Vec<u16> = "D:P(A;OICI;FA;;;OW)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        // SAFETY: terminated constant string and writable output pointer live through the call.
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
        Ok(Self(descriptor))
    }
}
impl Drop for PrivateSecurity {
    fn drop(&mut self) {
        // SAFETY: successful SDDL conversion owns this LocalAlloc allocation exactly once.
        unsafe {
            LocalFree(self.0);
        }
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
            security.0,
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
        let name = child_name(name)?;
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let bytes = offset + name.len() * size_of::<u16>();
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: usize storage supplies SDK alignment and the full header plus counted
        // UTF-16 tail, initialized to zero. Both handles and buffers outlive the call.
        // The source handle owns DELETE access. No full path or stream syntax is admitted.
        let ok = unsafe {
            (*info).Anonymous.ReplaceIfExists = true;
            (*info).RootDirectory = self.0.as_raw_handle();
            (*info).FileNameLength = (name.len() * 2) as u32;
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                ptr::addr_of_mut!((*info).FileName).cast(),
                name.len(),
            );
            SetFileInformationByHandle(
                source.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                bytes as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
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
