//! Registering the host with the browsers: their manifests (Chromium's, Firefox's), the registry
//! keys on Windows and the folders elsewhere, and the packaged host's manifests.

use super::*;

/// Where each Chromium browser looks for native messaging hosts on Windows, under HKCU (one
/// user) or HKLM (everyone). Chromium-branded builds (Chromium, Helium, Thorium, and Brave and
/// Brave Origin with every channel) read Chromium's first, then Chrome's, as most other forks do
/// (Chromium's launch_context_win.cc). Brave's own key is kept for builds that read it.
#[cfg(windows)]
pub(super) const REGISTRY_KEYS: [&str; 4] = [
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Chromium\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
];

/// Where Firefox looks.
#[cfg(windows)]
pub(super) const FIREFOX_KEY: &str = r"Software\Mozilla\NativeMessagingHosts";

/// Chrome's external extensions: an entry naming the store's extension makes Chrome offer it at
/// its next start ("New extension added"), the user turning it on there. HKLM's is read from the
/// 32-bit view on 64-bit Windows, hence /reg:32 (HKCU has one view). Helium reads it too, but
/// asks Google's address without the browser's version, and Google answers "no update": nothing
/// is offered there (#685), the app's window opens the store's page in Helium instead.
#[cfg(windows)]
pub(super) const CHROME_EXTERNAL_KEY: &str = r"Software\Google\Chrome\Extensions";

/// Linux: where the .deb and .rpm install this program, which their manifests name.
#[cfg(target_os = "linux")]
pub(super) const PACKAGED_HOST: &str = "/usr/bin/dannexdownloadmanager-nmhost";

/// This program, which the manifests name.
pub(super) fn this_program() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

/// The manifest Chromium browsers read: the host (`program`) may serve our extension's origin
/// only.
pub(super) fn chromium_manifest(program: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "name": HOST_NAME,
        "description": "Dannex Download Manager",
        "path": program,
        "type": "stdio",
        "allowed_origins": chromium_ids().into_iter().map(origin).collect::<Vec<_>>(),
    })
}

/// Firefox's manifest: the same host, allowed for our add-on ID instead of an origin.
pub(super) fn firefox_manifest(program: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "name": HOST_NAME,
        "description": "Dannex Download Manager",
        "path": program,
        "type": "stdio",
        "allowed_extensions": [FIREFOX_ID],
    })
}

pub(super) fn write_json(path: &std::path::Path, value: &serde_json::Value) -> Result<(), String> {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

pub(super) fn portable_refused() -> Result<(), String> {
    if mdm_ipc::portable_dir().is_some() {
        return Err(
            "portable mode: browser integration is not registered (nothing is written outside \
             its folder); install Dannex Download Manager to use it"
                .into(),
        );
    }
    Ok(())
}

/// Windows: the manifests beside this exe, and the registry entries naming them, per user
/// (HKCU) or for everyone (HKLM).
#[cfg(windows)]
pub(super) fn register(all_users: bool) -> Result<(), String> {
    portable_refused()?;
    let root = if all_users { "HKLM" } else { "HKCU" };
    let path = exe_dir()?.join(format!("{HOST_NAME}.json"));
    write_json(&path, &chromium_manifest(&this_program()?))?;
    for key in REGISTRY_KEYS {
        reg(&[
            "add",
            &format!(r"{root}\{key}\{HOST_NAME}"),
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &path.display().to_string(),
            "/f",
        ])?;
    }
    let ff_path = exe_dir()?.join(format!("{HOST_NAME}.firefox.json"));
    write_json(&ff_path, &firefox_manifest(&this_program()?))?;
    reg(&[
        "add",
        &format!(r"{root}\{FIREFOX_KEY}\{HOST_NAME}"),
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        &ff_path.display().to_string(),
        "/f",
    ])?;
    // Chrome offers the store's extension once its listing is public (never before: Chrome
    // would ask the store for one it doesn't show yet).
    if let Some(id) = listed_store_id() {
        reg(&[
            "add",
            &format!(r"{root}\{CHROME_EXTERNAL_KEY}\{id}"),
            "/v",
            "update_url",
            "/t",
            "REG_SZ",
            "/d",
            mdm_ipc::extension::STORE_UPDATE_URL,
            "/f",
            "/reg:32",
        ])?;
    }
    println!(
        "registered {HOST_NAME} for Chrome, Edge, Brave, Helium and the other Chromium browsers \
         ({}), and for Firefox, Zen and its other forks ({})",
        path.display(),
        ff_path.display()
    );
    Ok(())
}

#[cfg(windows)]
pub(super) fn unregister(all_users: bool) -> Result<(), String> {
    let root = if all_users { "HKLM" } else { "HKCU" };
    for key in REGISTRY_KEYS.into_iter().chain([FIREFOX_KEY]) {
        let _ = reg(&["delete", &format!(r"{root}\{key}\{HOST_NAME}"), "/f"]);
    }
    let _ = reg(&[
        "delete",
        &format!(r"{root}\{CHROME_EXTERNAL_KEY}\{STORE_ID}"),
        "/f",
        "/reg:32",
    ]);
    let _ = std::fs::remove_file(exe_dir()?.join(format!("{HOST_NAME}.json")));
    let _ = std::fs::remove_file(exe_dir()?.join(format!("{HOST_NAME}.firefox.json")));
    println!("unregistered {HOST_NAME}");
    Ok(())
}

#[cfg(windows)]
pub(super) fn reg(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Linux and macOS: the folders the browsers there read host manifests from, for one user (in
/// the home folder) or for everyone, each with whether it is Firefox's (`mdm_ipc::browsers`).
/// With `host`, a user's folder is left out for a browser that reads a manifest for everyone
/// naming it (the package's, #266).
#[cfg(not(windows))]
pub(super) fn host_dirs(all_users: bool, host: Option<&std::path::Path>) -> Vec<(PathBuf, bool)> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    mdm_ipc::browsers::host_dirs(&home, &config, cfg!(target_os = "macos"), all_users, host)
        .into_iter()
        .map(|(d, f)| (d, f == mdm_ipc::browsers::Family::Gecko))
        .collect()
}

/// Linux and macOS: the folders the browsers there read external extensions from (an
/// `<id>.json` naming the store's update service makes the browser offer that extension at its
/// next start), for those that are there, each with the service's address as that browser names
/// it (`mdm_ipc::browsers::external_dirs`: Chrome's, and Helium's own, #685). Linux Chrome reads
/// only the system's, so a per-user registration there leaves Chrome to the app's own offer
/// (extension.rs).
#[cfg(not(windows))]
pub(super) fn external_dirs(all_users: bool) -> Vec<(PathBuf, &'static str)> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    mdm_ipc::browsers::external_dirs(&home, &config, cfg!(target_os = "macos"), all_users)
}

/// Linux: this program runs from inside an AppImage's mount (`APPDIR`), which is gone once the
/// AppImage quits: a manifest naming it would name a program that no longer exists (#253). The
/// app registers a copy outside it instead (unix.rs).
#[cfg(not(windows))]
pub(super) fn mount_refused() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    match std::env::var_os("APPDIR").map(PathBuf::from) {
        Some(mount) if !mount.as_os_str().is_empty() && exe.starts_with(&mount) => Err(format!(
            "{} is inside the AppImage's mount, which is gone once it quits: register a copy \
             outside it",
            exe.display()
        )),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
pub(super) fn register(all_users: bool) -> Result<(), String> {
    portable_refused()?;
    mount_refused()?;
    let program = this_program()?;
    let (chromium, firefox) = (chromium_manifest(&program), firefox_manifest(&program));
    let dirs = host_dirs(all_users, Some(&program));
    // A browser that reads the package's manifest for everyone, naming this program, needs none
    // in the home folder: one written there before is taken away, so uninstalling the package
    // leaves nothing behind (#266). The browser reads a user's manifest first, so one naming
    // another program (an earlier DDM, an AppImage's copy) would win over the package's: the DDM
    // registered last is the one the browsers start, as when each wrote its own.
    let mut covered = 0;
    if !all_users {
        for (dir, _) in host_dirs(false, None) {
            if dirs.iter().any(|(d, _)| *d == dir) {
                continue;
            }
            covered += 1;
            let manifest = dir.join(format!("{HOST_NAME}.json"));
            if std::fs::remove_file(&manifest).is_ok() {
                println!(
                    "removed {} (the browser reads the one for everyone)",
                    manifest.display()
                );
            }
        }
    }
    if covered > 0 {
        println!(
            "registered {HOST_NAME} for everyone by the package: {covered} of this user's \
             browser folders need nothing"
        );
    }
    // One browser's folder that can't be written doesn't keep the others from theirs.
    let mut failed = Vec::new();
    for (dir, is_firefox) in &dirs {
        let manifest = if *is_firefox { &firefox } else { &chromium };
        let written = std::fs::create_dir_all(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))
            .and_then(|()| write_json(&dir.join(format!("{HOST_NAME}.json")), manifest));
        match written {
            Ok(()) => println!("registered {HOST_NAME} in {}", dir.display()),
            Err(e) => failed.push(e),
        }
    }
    if dirs.is_empty() && covered == 0 {
        println!("registered {HOST_NAME} for no browser: none found");
    }
    if !failed.is_empty() {
        return Err(failed.join("; "));
    }
    // Linux, the .deb's or .rpm's helper for one user: no offer in the home folder, which the
    // package manager couldn't take away with the package (#266); the app's window offers it.
    #[cfg(target_os = "linux")]
    let offer = all_users || program != std::path::Path::new(PACKAGED_HOST);
    #[cfg(not(target_os = "linux"))]
    let offer = true;
    if let Some(id) = listed_store_id().filter(|_| offer) {
        for (dir, update_url) in external_dirs(all_users) {
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            write_json(
                &dir.join(format!("{id}.json")),
                &serde_json::json!({ "external_update_url": update_url }),
            )?;
            println!("offered the extension in {}", dir.display());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn unregister(all_users: bool) -> Result<(), String> {
    for (dir, _) in host_dirs(all_users, None) {
        let _ = std::fs::remove_file(dir.join(format!("{HOST_NAME}.json")));
    }
    for (dir, _) in external_dirs(all_users) {
        let _ = std::fs::remove_file(dir.join(format!("{STORE_ID}.json")));
    }
    println!("unregistered {HOST_NAME}");
    Ok(())
}

/// Linux (the .deb and .rpm build, tools/installer/build-linux.sh): the manifests for everyone,
/// in each folder the browsers read them from (`mdm_ipc::browsers::LINUX_SYSTEM_HOST_DIRS`),
/// under `root`, naming the packaged helper. The package owns them, so the package manager
/// removes them with it (#266).
#[cfg(target_os = "linux")]
pub(super) fn package_manifests(root: Option<&str>) -> Result<(), String> {
    let root = std::path::Path::new(root.ok_or("--package-manifests needs a folder")?);
    let program = std::path::Path::new(PACKAGED_HOST);
    for (dir, family) in mdm_ipc::browsers::LINUX_SYSTEM_HOST_DIRS {
        let dir = root.join(dir.trim_start_matches('/'));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let manifest = match family {
            mdm_ipc::browsers::Family::Chromium => chromium_manifest(program),
            mdm_ipc::browsers::Family::Gecko => firefox_manifest(program),
        };
        let path = dir.join(format!("{HOST_NAME}.json"));
        write_json(&path, &manifest)?;
        println!("{}", path.display());
    }
    Ok(())
}

/// The system-wide manifests are packaged for Linux only (macOS's and Windows' installers
/// register with `--register --all-users`).
#[cfg(not(target_os = "linux"))]
pub(super) fn package_manifests(_: Option<&str>) -> Result<(), String> {
    Err("--package-manifests: the .deb and .rpm are Linux's".into())
}
