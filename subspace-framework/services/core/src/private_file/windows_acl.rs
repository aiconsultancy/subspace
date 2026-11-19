//! Owner-only Windows DACL operations for private files.
// qual:allow(srp, max_independent_clusters=4) reason: "The moved Windows FFI wrappers share one security-descriptor boundary; checked pointer provenance and allocation ownership stay beside DACL validation."
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};
mod identity;
use identity::{current_user, sid_string};

pub fn set_owner_only(path: &Path, inheritable: bool) -> std::io::Result<()> {
    install_owner_only(path, inheritable).map_err(|()| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "cannot set owner-only ACL",
        )
    })
}

pub fn verify_owner_only(path: &Path, protected: bool, inherited: bool) -> std::io::Result<()> {
    check_owner_only(path, protected, inherited).map_err(|()| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "file ACL is not owner-only",
        )
    })
}

// qual:allow(complexity, unsafe) reason: "Narrow Windows security-descriptor FFI wrapper; every pointer lifetime and allocation owner is checked inline."
fn install_owner_only(path: &Path, inheritable: bool) -> Result<(), ()> {
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    let user = current_user()?;
    let sid = sid_string(user.sid())?;
    let flags = if inheritable { "OICI" } else { "" };
    let sddl: Vec<u16> = OsStr::new(&format!("D:P(A;{flags};FA;;;{sid})"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: pointers are valid for the duration of the call and the API allocates descriptor.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(());
    }
    let result = apply_descriptor(path, descriptor);
    // SAFETY: descriptor was allocated by LocalAlloc through the conversion API.
    unsafe { LocalFree(descriptor.cast()) };
    result
}

// qual:allow(complexity, unsafe) reason: "Narrow Windows DACL installation wrapper with checked API status and documented pointer provenance."
fn apply_descriptor(path: &Path, descriptor: PSECURITY_DESCRIPTOR) -> Result<(), ()> {
    use windows_sys::Win32::Security::GetSecurityDescriptorDacl;
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = ptr::null_mut();
    // SAFETY: descriptor is valid and output pointers refer to local variables.
    let ok =
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) };
    if ok == 0 || present == 0 {
        return Err(());
    }
    let mut wide = wide_path(path);
    // SAFETY: path is NUL-terminated and dacl came from a valid descriptor.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        )
    };
    (status == ERROR_SUCCESS).then_some(()).ok_or(())
}

// qual:allow(complexity, unsafe) reason: "Narrow Windows security-descriptor read wrapper; the descriptor is released through LocalFree."
fn check_owner_only(path: &Path, protected: bool, inherited: bool) -> Result<(), ()> {
    let mut owner = ptr::null_mut();
    let mut group = ptr::null_mut();
    let mut dacl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let mut wide = wide_path(path);
    // SAFETY: output pointers are valid and the path is NUL-terminated.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            &mut group,
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS || owner.is_null() || dacl.is_null() {
        return Err(());
    }
    let result = descriptor_is_private(descriptor, dacl, owner, protected, inherited);
    // SAFETY: descriptor was allocated by the security API.
    unsafe { LocalFree(descriptor.cast()) };
    result
}

// qual:allow(complexity, unsafe) reason: "Audited Windows ACL parser verifies protection, current-user ownership, and exactly one allow ACE."
fn descriptor_is_private(
    descriptor: PSECURITY_DESCRIPTOR,
    dacl: *mut windows_sys::Win32::Security::ACL,
    owner: PSID,
    protected: bool,
    inherited: bool,
) -> Result<(), ()> {
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, EqualSid, GetAce, GetSecurityDescriptorControl, SE_DACL_PROTECTED,
    };
    const INHERITED_ACE_FLAG: u8 = 0x10;
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: descriptor is valid and outputs point to local variables.
    let control_ok =
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0;
    if !control_ok || (control & SE_DACL_PROTECTED != 0) != protected {
        return Err(());
    }
    let user = current_user()?;
    // SAFETY: both pointers refer to valid SIDs during this call.
    if unsafe { EqualSid(owner, user.sid()) } == 0 {
        return Err(());
    }
    // SAFETY: caller supplies an ACL returned by GetNamedSecurityInfoW.
    let count = unsafe { (*dacl).AceCount };
    if count != 1 {
        return Err(());
    }
    let mut ace = ptr::null_mut();
    // SAFETY: index zero is valid because the ACL reports one ACE.
    if unsafe { GetAce(dacl, 0, &mut ace) } == 0 {
        return Err(());
    }
    let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
    // SAFETY: every ACE begins with an ACE_HEADER.
    if unsafe { (*allowed).Header.AceType } != 0 {
        return Err(());
    }
    // SAFETY: every ACE begins with an ACE_HEADER.
    if (unsafe { (*allowed).Header.AceFlags } & INHERITED_ACE_FLAG != 0) != inherited {
        return Err(());
    }
    // SAFETY: the only accepted ACE is interpreted as ACCESS_ALLOWED_ACE.
    let sid = unsafe { ptr::addr_of_mut!((*allowed).SidStart).cast() };
    // SAFETY: both pointers refer to valid SIDs during this call.
    (unsafe { EqualSid(user.sid(), sid) } != 0)
        .then_some(())
        .ok_or(())
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
