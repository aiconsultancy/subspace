use std::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::PSID;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(super) struct CurrentUser(Vec<u8>);

impl CurrentUser {
    pub(super) fn sid(&self) -> PSID {
        use windows_sys::Win32::Security::TOKEN_USER;
        let user = self.0.as_ptr().cast::<TOKEN_USER>();
        // SAFETY: the buffer was populated as TOKEN_USER and remains alive.
        unsafe { (*user).User.Sid }
    }
}

// qual:allow(complexity, unsafe) reason: "Audited Windows token query wrapper owns its handle and keeps the SID buffer alive."
pub(super) fn current_user() -> Result<CurrentUser, ()> {
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TokenUser};
    let mut token = ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle and token is an output.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(());
    }
    let mut needed = 0;
    // SAFETY: null data with zero length is the documented size query.
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed) };
    let mut data = vec![0_u8; needed as usize];
    // SAFETY: data has the requested size and output pointers are valid.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    // SAFETY: token came from OpenProcessToken.
    unsafe { CloseHandle(token) };
    (ok != 0).then_some(CurrentUser(data)).ok_or(())
}

// qual:allow(complexity, unsafe) reason: "Audited Windows SID conversion wrapper bounds the UTF-16 scan and releases the platform allocation."
pub(super) fn sid_string(sid: PSID) -> Result<String, ()> {
    let mut text = ptr::null_mut();
    // SAFETY: sid is valid and text receives a LocalAlloc-owned string.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(());
    }
    let mut length = 0;
    // SAFETY: text points to a NUL-terminated string allocated by Windows.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the preceding scan found the valid string length.
    let value =
        String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).map_err(|_| ());
    // SAFETY: text was allocated by ConvertSidToStringSidW.
    unsafe { LocalFree(text.cast()) };
    value
}
