//! Sign-in secrets in Windows Credential Manager (#44), and DPAPI's protection of a secret.

/// Options → Sites Logins' passwords, encrypted for this Windows account (DPAPI) before the list
/// is written: another account, or a copy of the file, can't read them.
#[allow(unsafe_code)]
pub fn protect(plain: &str) -> Option<Vec<u8>> {
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plain.len()).ok()?,
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `plain` for the call (only read); `output` is filled by the
    // system with a buffer we copy and then free with LocalFree, as CryptProtectData requires.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    blob_bytes(ok, output)
}

/// The password `protect` encrypted, for this account.
#[allow(unsafe_code)]
pub fn unprotect(sealed: &[u8]) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(sealed.len()).ok()?,
        pbData: sealed.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: as in `protect`.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    String::from_utf8(blob_bytes(ok, output)?).ok()
}

/// A DPAPI result's bytes, its buffer freed.
#[allow(unsafe_code)]
pub(super) fn blob_bytes(
    ok: i32,
    blob: windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB,
) -> Option<Vec<u8>> {
    if ok == 0 || blob.pbData.is_null() {
        return None;
    }
    // SAFETY: the system gave `cbData` bytes at `pbData`, ours until LocalFree.
    let bytes = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
    // SAFETY: the buffer came from DPAPI, which allocates with LocalAlloc.
    unsafe { windows_sys::Win32::Foundation::LocalFree(blob.pbData.cast()) };
    Some(bytes)
}

// Sign-in secrets in Windows Credential Manager (#44, secrets.rs): generic credentials named
// `<service>/<account>`, for this Windows user, roaming with the profile as DDM's data folder
// (%APPDATA%) does. Credential Manager (Control Panel → Credential Manager → Windows
// Credentials) shows them, the label as their user name, and can remove them.

pub(super) fn cred_target(service: &str, account: &str) -> Vec<u16> {
    format!("{service}/{account}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// The last error as a fault: no logon session (a service account) means no keychain.
#[allow(unsafe_code)]
pub(super) fn cred_fault(what: &str) -> crate::secrets::Fault {
    use crate::secrets::Fault;
    use windows_sys::Win32::Foundation::{ERROR_NO_SUCH_LOGON_SESSION, GetLastError};
    // SAFETY: no arguments; reads this thread's last error.
    let code = unsafe { GetLastError() };
    if code == ERROR_NO_SUCH_LOGON_SESSION {
        Fault::Missing
    } else {
        Fault::Failed(format!("{what}: Windows error {code}"))
    }
}

#[allow(unsafe_code)]
pub(super) fn cred_not_found() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    // SAFETY: no arguments.
    unsafe { GetLastError() == ERROR_NOT_FOUND }
}

/// Whether Credential Manager answers (listing DDM's own entries).
pub fn keychain_probe(service: &str) -> Result<(), crate::secrets::Fault> {
    keychain_list(service).map(|_| ())
}

#[allow(unsafe_code)]
pub fn keychain_read(
    service: &str,
    account: &str,
) -> Result<Option<String>, crate::secrets::Fault> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };
    let target = cred_target(service, account);
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `target` is a NUL-terminated UTF-16 string alive for the call; `cred` receives a
    // buffer the system allocated, freed below with CredFree.
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
    if ok == 0 {
        return if cred_not_found() {
            Ok(None)
        } else {
            Err(cred_fault("CredReadW"))
        };
    }
    // SAFETY: on success `cred` points at a CREDENTIALW whose blob holds `CredentialBlobSize`
    // bytes; both are ours until CredFree.
    let bytes = unsafe {
        let c = &*cred;
        let bytes = if c.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        };
        CredFree(cred.cast());
        bytes
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| crate::secrets::Fault::Failed("not UTF-8".into()))
}

/// Stored as UTF-8. Credential Manager takes at most 2560 bytes: a longer secret stays in the
/// settings, sealed with DPAPI.
#[allow(unsafe_code)]
pub fn keychain_write(
    service: &str,
    account: &str,
    label: &str,
    secret: &str,
) -> Result<(), crate::secrets::Fault> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredWriteW,
    };
    if secret.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err(crate::secrets::Fault::Failed(
            "longer than Credential Manager takes".into(),
        ));
    }
    let mut target = cred_target(service, account);
    let mut user: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
    let mut blob = secret.as_bytes().to_vec();
    // SAFETY: all-zero is a valid CREDENTIALW (null pointers, zero sizes).
    let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = target.as_mut_ptr();
    cred.UserName = user.as_mut_ptr();
    cred.CredentialBlobSize = blob.len() as u32;
    cred.CredentialBlob = blob.as_mut_ptr();
    cred.Persist = CRED_PERSIST_ENTERPRISE;
    // SAFETY: every pointer in `cred` points at a buffer alive for the call (only read).
    let ok = unsafe { CredWriteW(&cred, 0) };
    let fault = (ok == 0).then(|| cred_fault("CredWriteW"));
    // The plain copy doesn't linger in this buffer.
    blob.iter_mut().for_each(|b| *b = 0);
    fault.map_or(Ok(()), Err)
}

#[allow(unsafe_code)]
pub fn keychain_delete(service: &str, account: &str) -> Result<(), crate::secrets::Fault> {
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
    let target = cred_target(service, account);
    // SAFETY: `target` is NUL-terminated and alive for the call.
    let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 && !cred_not_found() {
        return Err(cred_fault("CredDeleteW"));
    }
    Ok(())
}

/// The accounts of `service`'s generic credentials (the filter `<service>/*`: only DDM's; the
/// others' blobs are never read).
#[allow(unsafe_code)]
pub fn keychain_list(service: &str) -> Result<Vec<String>, crate::secrets::Fault> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree,
    };
    let filter: Vec<u16> = format!("{service}/*")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut count = 0u32;
    let mut creds: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `filter` is NUL-terminated; `creds` receives an array of `count` pointers the
    // system allocated, freed below with CredFree.
    let ok = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut creds) };
    if ok == 0 {
        return if cred_not_found() {
            Ok(Vec::new())
        } else {
            Err(cred_fault("CredEnumerateW"))
        };
    }
    let prefix = format!("{service}/");
    let mut accounts = Vec::new();
    // SAFETY: on success `creds` holds `count` valid CREDENTIALW pointers, ours until CredFree;
    // each TargetName is a NUL-terminated UTF-16 string.
    unsafe {
        for i in 0..count as usize {
            let c = &**creds.add(i);
            if c.Type != CRED_TYPE_GENERIC || c.TargetName.is_null() {
                continue;
            }
            let mut n = 0;
            while *c.TargetName.add(n) != 0 {
                n += 1;
            }
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(c.TargetName, n));
            if let Some(account) = name.strip_prefix(&prefix) {
                accounts.push(account.to_owned());
            }
        }
        CredFree(creds.cast());
    }
    Ok(accounts)
}
