use std::path::PathBuf;

#[cfg(windows)]
pub(super) fn documents_known_folder() -> Option<PathBuf> {
    use std::ffi::{OsString, c_void};
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };

    let mut wpath: windows_sys::core::PWSTR = ptr::null_mut();
    // SAFETY: FFI call into the Shell known-folder API. `wpath` is an out
    // parameter the callee allocates via `CoTaskMemAlloc`; we free it via
    // `CoTaskMemFree` on every exit path (including error).
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_Documents,
            KF_FLAG_DEFAULT as u32,
            ptr::null_mut(),
            &mut wpath,
        )
    };
    if hr < 0 || wpath.is_null() {
        if !wpath.is_null() {
            unsafe { CoTaskMemFree(wpath.cast::<c_void>()) };
        }
        return None;
    }
    let mut len = 0isize;
    // SAFETY: SHGetKnownFolderPath returned a NUL-terminated UTF-16 buffer.
    while unsafe { *wpath.offset(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` bounds the buffer up to (but not including) the NUL.
    let slice = unsafe { std::slice::from_raw_parts(wpath, len as usize) };
    let os = OsString::from_wide(slice);
    unsafe { CoTaskMemFree(wpath.cast::<c_void>()) };
    let path = PathBuf::from(os);
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

#[cfg(not(windows))]
pub(super) fn documents_known_folder() -> Option<PathBuf> {
    None
}
