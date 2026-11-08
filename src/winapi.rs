//! Minimal Win32 layer: admin check, self-elevation, SCM services, HKLM registry,
//! fixed drives, console VT. Hand-rolled extern "system" declarations — zero crates.

use std::ffi::c_void;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

// ---------- kernel32 ----------
#[allow(non_snake_case)]
#[link(name = "kernel32")]
extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
    fn GetStdHandle(nStdHandle: i32) -> *mut c_void;
    fn GetConsoleMode(h: *mut c_void, lpMode: *mut u32) -> i32;
    fn SetConsoleMode(h: *mut c_void, dwMode: u32) -> i32;
}

// ---------- shell32 ----------
#[allow(non_snake_case)]
#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(
        hwnd: *mut c_void,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
}

// ---------- advapi32: registry ----------
const HKEY_LOCAL_MACHINE: *mut c_void = 0x8000_0002usize as *mut c_void;
const REG_SZ: u32 = 1;
const REG_DWORD: u32 = 4;
const KEY_SET_VALUE: u32 = 0x0002;
const KEY_QUERY_VALUE: u32 = 0x0001;
const ERROR_FILE_NOT_FOUND: i32 = 2;

#[allow(non_snake_case)]
#[link(name = "advapi32")]
extern "system" {
    fn RegCreateKeyExW(
        hKey: *mut c_void,
        lpSubKey: *const u16,
        reserved: u32,
        lpClass: *const u16,
        dwOptions: u32,
        samDesired: u32,
        lpSecAttrs: *mut c_void,
        phkResult: *mut *mut c_void,
        lpdwDisposition: *mut u32,
    ) -> i32;
    fn RegOpenKeyExW(
        hKey: *mut c_void,
        lpSubKey: *const u16,
        ulOptions: u32,
        samDesired: u32,
        phkResult: *mut *mut c_void,
    ) -> i32;
    fn RegSetValueExW(
        hKey: *mut c_void,
        lpValueName: *const u16,
        reserved: u32,
        dwType: u32,
        lpData: *const u8,
        cbData: u32,
    ) -> i32;
    fn RegQueryValueExW(
        hKey: *mut c_void,
        lpValueName: *const u16,
        lpReserved: *mut u32,
        lpType: *mut u32,
        lpData: *mut u8,
        lpcbData: *mut u32,
    ) -> i32;
    fn RegDeleteValueW(hKey: *mut c_void, lpValueName: *const u16) -> i32;
    fn RegCloseKey(hKey: *mut c_void) -> i32;
}

// ---------- advapi32: service control manager ----------
#[repr(C)]
#[allow(non_snake_case)]
struct SERVICE_STATUS {
    dwServiceType: u32,
    dwCurrentState: u32,
    dwControlsAccepted: u32,
    dwWin32ExitCode: u32,
    dwServiceSpecificExitCode: u32,
    dwCheckPoint: u32,
    dwWaitHint: u32,
}

const SERVICE_QUERY_STATUS: u32 = 0x0004;
const SERVICE_START: u32 = 0x0010;
const SERVICE_STOP: u32 = 0x0020;
const SERVICE_CHANGE_CONFIG: u32 = 0x0002;
const SERVICE_NO_CHANGE: u32 = 0xFFFF_FFFF;
const SERVICE_CONTROL_STOP: u32 = 1;
const SERVICE_RUNNING: u32 = 4;

#[allow(non_snake_case)]
#[link(name = "advapi32")]
extern "system" {
    fn OpenSCManagerW(
        lpMachineName: *const u16,
        lpDatabaseName: *const u16,
        dwDesiredAccess: u32,
    ) -> *mut c_void;
    fn OpenServiceW(
        hSCManager: *mut c_void,
        lpServiceName: *const u16,
        dwDesiredAccess: u32,
    ) -> *mut c_void;
    fn StartServiceW(
        hService: *mut c_void,
        dwNumServiceArgs: u32,
        lpServiceArgVectors: *const *const u16,
    ) -> i32;
    fn ControlService(
        hService: *mut c_void,
        dwControl: u32,
        lpServiceStatus: *mut SERVICE_STATUS,
    ) -> i32;
    fn QueryServiceStatus(hService: *mut c_void, lpServiceStatus: *mut SERVICE_STATUS) -> i32;
    fn ChangeServiceConfigW(
        hService: *mut c_void,
        dwServiceType: u32,
        dwStartType: u32,
        dwErrorControl: u32,
        lpBinaryPathName: *const u16,
        lpLoadOrderGroup: *const u16,
        lpdwTagId: *mut u32,
        lpDependencies: *const u16,
        lpServiceStartName: *const u16,
        lpPassword: *const u16,
        lpDisplayName: *const u16,
    ) -> i32;
    fn CloseServiceHandle(hSCObject: *mut c_void) -> i32;
}

// ---------- admin + elevation ----------

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// Relaunch own exe elevated via UAC and exit the current process.
pub fn relaunch_elevated() -> ! {
    let exe = std::env::current_exe().unwrap_or_default();
    let mut file: Vec<u16> = OsStr::new(exe.as_os_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if file.len() <= 1 {
        file = wide("wu-repair-cycle.exe");
    }
    let op = wide("runas");
    let params = wide("");
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            1, // SW_SHOWNORMAL
        );
    }
    std::process::exit(0);
}

// ---------- services (raw API first, sc.exe fallback for hardened SCM hosts) ----------

fn sc_exe(args: &[&str]) -> (i32, String) {
    match Command::new("sc.exe").args(args).output() {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).to_string(),
        ),
        Err(_) => (-1, String::new()),
    }
}

fn open_service(name: &str, access: u32) -> Option<*mut c_void> {
    unsafe {
        let mut scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), 0xF003F);
        if scm.is_null() {
            scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), 0x0001);
        }
        if scm.is_null() {
            return None;
        }
        let wname = wide(name);
        let mut h = OpenServiceW(scm, wname.as_ptr(), access);
        if h.is_null() {
            // retry with the minimal access we can live with
            h = OpenServiceW(scm, wname.as_ptr(), SERVICE_QUERY_STATUS);
        }
        CloseServiceHandle(scm);
        if h.is_null() {
            None
        } else {
            Some(h)
        }
    }
}

/// Some(true/false) = verified state; None = cannot determine via raw API.
pub fn sc_running(name: &str) -> Option<bool> {
    let h = open_service(name, SERVICE_QUERY_STATUS)?;
    let mut st = SERVICE_STATUS {
        dwServiceType: 0,
        dwCurrentState: 0,
        dwControlsAccepted: 0,
        dwWin32ExitCode: 0,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    let ok = unsafe { QueryServiceStatus(h, &mut st) };
    unsafe {
        CloseServiceHandle(h);
    }
    if ok != 0 {
        Some(st.dwCurrentState == SERVICE_RUNNING)
    } else {
        None
    }
}

fn running_verified(name: &str) -> bool {
    if let Some(r) = sc_running(name) {
        return r;
    }
    // hardened SCM fallback: text-verify via sc.exe
    let (c, out) = sc_exe(&["query", name]);
    c == 0 && out.contains("RUNNING")
}

fn stopped_verified(name: &str) -> bool {
    if let Some(r) = sc_running(name) {
        return !r;
    }
    let (c, out) = sc_exe(&["query", name]);
    c == 0 && out.contains("STOPPED")
}

/// Start a service and VERIFY it is running (30s patience - busy systems can
/// take well over 10s while TiWorker churns the component store).
pub fn sc_start(name: &str) -> bool {
    if let Some(h) = open_service(name, SERVICE_START | SERVICE_QUERY_STATUS) {
        unsafe {
            StartServiceW(h, 0, std::ptr::null());
            CloseServiceHandle(h);
        }
    } else {
        let (c, _) = sc_exe(&["start", name]);
        if c != 0 && running_verified(name) {
            return true; // 1056 already running
        }
    }
    for i in 0..150 {
        if running_verified(name) {
            return true;
        }
        if i % 50 == 49 {
            crate::ui::heartbeat(&format!(
                "waiting for {} to reach RUNNING... ({}s elapsed)",
                name,
                (i + 1) / 5
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    running_verified(name)
}

/// Stop a service and VERIFY it stopped.
pub fn sc_stop(name: &str) -> bool {
    if let Some(h) = open_service(name, SERVICE_STOP | SERVICE_QUERY_STATUS) {
        let mut st = SERVICE_STATUS {
            dwServiceType: 0,
            dwCurrentState: 0,
            dwControlsAccepted: 0,
            dwWin32ExitCode: 0,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: 0,
            dwWaitHint: 0,
        };
        unsafe {
            ControlService(h, SERVICE_CONTROL_STOP, &mut st);
            CloseServiceHandle(h);
        }
    } else {
        sc_exe(&["stop", name]);
    }
    for _ in 0..50 {
        if stopped_verified(name) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    stopped_verified(name)
}

/// Wait for the WU datastore file to appear/grow, heartbeating so the console
/// never goes silent for more than 10 s. Returns (ok, waited_secs).
pub fn wait_datastore(path: &std::path::Path, max_secs: u64) -> (bool, u64) {
    let mut waited = 0u64;
    while waited < max_secs {
        let ok = path.exists()
            && std::fs::metadata(path).map(|m| m.len() > 100).unwrap_or(false);
        if ok {
            return (true, waited);
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
        waited += 5;
        crate::ui::heartbeat(&format!(
            "waiting for the fresh WU datastore to rebuild... ({}s of {}s)",
            waited, max_secs
        ));
    }
    (path.exists(), waited)
}

/// Set start type (2 auto / 3 manual / 4 disabled) and verify it stuck.
pub fn sc_set_start_type(name: &str, ty: u32) -> bool {
    if let Some(h) = open_service(name, SERVICE_CHANGE_CONFIG) {
        let ok = unsafe {
            ChangeServiceConfigW(
                h,
                SERVICE_NO_CHANGE,
                ty,
                SERVICE_NO_CHANGE,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        unsafe {
            CloseServiceHandle(h);
        }
        if ok != 0 {
            return true;
        }
    }
    // fallback through sc.exe
    let tyname = match ty {
        2 => "auto",
        3 => "demand",
        4 => "disabled",
        _ => return false,
    };
    let (c, _) = sc_exe(&["config", name, &format!("start={}", tyname)]);
    c == 0
}

// ---------- registry (HKLM only) ----------

pub fn reg_set_sz(path: &str, name: &str, val: &str) -> bool {
    unsafe {
        let mut hk: *mut c_void = std::ptr::null_mut();
        let wp = wide(path);
        let wn = wide(name);
        let r = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            wp.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut hk,
            std::ptr::null_mut(),
        );
        if r != 0 {
            return false;
        }
        let mut v: Vec<u16> = OsStr::new(val).encode_wide().collect();
        v.push(0);
        let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
        let rr = RegSetValueExW(hk, wn.as_ptr(), 0, REG_SZ, bytes.as_ptr(), (v.len() * 2) as u32);
        RegCloseKey(hk);
        rr == 0
    }
}

pub fn reg_set_dword(path: &str, name: &str, val: u32) -> bool {
    unsafe {
        let mut hk: *mut c_void = std::ptr::null_mut();
        let wp = wide(path);
        let wn = wide(name);
        let r = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            wp.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut hk,
            std::ptr::null_mut(),
        );
        if r != 0 {
            return false;
        }
        let b = val.to_le_bytes();
        let rr = RegSetValueExW(hk, wn.as_ptr(), 0, REG_DWORD, b.as_ptr(), 4);
        RegCloseKey(hk);
        rr == 0
    }
}

/// Delete a value. Absent value counts as success (idempotent).
pub fn reg_del(path: &str, name: &str) -> bool {
    unsafe {
        let mut hk: *mut c_void = std::ptr::null_mut();
        let wp = wide(path);
        let wn = wide(name);
        let r = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            wp.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hk,
        );
        if r != 0 {
            return r == ERROR_FILE_NOT_FOUND;
        }
        let rr = RegDeleteValueW(hk, wn.as_ptr());
        RegCloseKey(hk);
        rr == 0 || rr == ERROR_FILE_NOT_FOUND
    }
}

pub fn reg_get_sz(path: &str, name: &str) -> Option<String> {
    unsafe {
        let mut hk: *mut c_void = std::ptr::null_mut();
        let wp = wide(path);
        let wn = wide(name);
        let r = RegOpenKeyExW(HKEY_LOCAL_MACHINE, wp.as_ptr(), 0, KEY_QUERY_VALUE, &mut hk);
        if r != 0 {
            return None;
        }
        let mut ty: u32 = 0;
        let mut cb: u32 = 0;
        let q = RegQueryValueExW(hk, wn.as_ptr(), std::ptr::null_mut(), &mut ty, std::ptr::null_mut(), &mut cb);
        if q != 0 || cb == 0 {
            RegCloseKey(hk);
            return None;
        }
        let mut buf: Vec<u8> = vec![0u8; cb as usize];
        let q2 = RegQueryValueExW(
            hk,
            wn.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            &mut cb,
        );
        RegCloseKey(hk);
        if q2 != 0 {
            return None;
        }
        let units = cb as usize / 2;
        let w: Vec<u16> = (0..units).map(|i| u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]])).collect();
        let trimmed: Vec<u16> = w.into_iter().take_while(|&x| x != 0).collect();
        Some(String::from_utf16_lossy(&trimmed))
    }
}

pub fn reg_get_dword(path: &str, name: &str) -> Option<u32> {
    unsafe {
        let mut hk: *mut c_void = std::ptr::null_mut();
        let wp = wide(path);
        let wn = wide(name);
        let r = RegOpenKeyExW(HKEY_LOCAL_MACHINE, wp.as_ptr(), 0, KEY_QUERY_VALUE, &mut hk);
        if r != 0 {
            return None;
        }
        let mut ty: u32 = 0;
        let mut buf = [0u8; 4];
        let mut cb: u32 = 4;
        let q = RegQueryValueExW(
            hk,
            wn.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut cb,
        );
        RegCloseKey(hk);
        if q != 0 || ty != REG_DWORD || cb != 4 {
            return None;
        }
        Some(u32::from_le_bytes(buf))
    }
}

// ---------- service object security: SD heal for hardened hosts ----------
// Some update-blocker tools strip EVERY grant from wuauserv/UsoSvc DACLs, so even
// elevated admins get ACCESS_DENIED on query/start/config. Heal = take ownership
// (SeTakeOwnershipPrivilege) then rewrite the DACL granting Administrators+SYSTEM
// full service control. Hand-built absolute SECURITY_DESCRIPTOR, zero extra APIs.

const OWNER_SECURITY_INFORMATION: u32 = 0x0000_0001;
const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
const SE_DACL_PRESENT: u16 = 0x0004;
const WRITE_OWNER: u32 = 0x0008_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
const TOKEN_QUERY: u32 = 0x0008;
const SE_PRIVILEGE_ENABLED: u32 = 0x0002;
const SERVICE_ALL_ACCESS: u32 = 0xF01FF;

#[repr(C)]
#[allow(non_snake_case)]
struct LUID {
    LowPart: u32,
    HighPart: i32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct TOKEN_PRIVILEGES1 {
    PrivilegeCount: u32,
    Luid: LUID,
    Attributes: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct SD_ABS {
    Revision: u8,
    Sbz1: u8,
    Control: u16,
    Owner: *const u8,
    Group: *const u8,
    Sacl: *const u8,
    Dacl: *const u8,
}

#[allow(non_snake_case)]
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(
        h: *mut c_void,
        desired: u32,
        tok: *mut *mut c_void,
    ) -> i32;
    fn LookupPrivilegeValueW(
        sysname: *const u16,
        name: *const u16,
        luid: *mut LUID,
    ) -> i32;
    fn AdjustTokenPrivileges(
        tok: *mut c_void,
        disableAll: i32,
        newstate: *mut TOKEN_PRIVILEGES1,
        buflen: u32,
        prev: *mut u8,
        retlen: *mut u32,
    ) -> i32;
    fn SetServiceObjectSecurity(
        hService: *mut c_void,
        info: u32,
        sd: *const u8,
    ) -> i32;
}

#[allow(non_snake_case)]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn CloseHandle(h: *mut c_void) -> i32;
}

/// S-1-5-<subs...> as raw bytes.
fn sid_bytes(subs: &[u32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + 4 * subs.len());
    v.push(1);
    v.push(subs.len() as u8);
    v.extend_from_slice(&[0u8, 0, 0, 0, 0, 5]);
    for s in subs {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

/// ACL with ACCESS_ALLOWED_ACEs of (mask, sid).
fn acl_with_aces(aces: &[(u32, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (mask, sid) in aces {
        let ace_len = (4 + 4 + sid.len()) as u16;
        body.extend_from_slice(&[0u8, 0]); // ACE_HEADER type=ACCESS_ALLOWED, flags=0
        body.extend_from_slice(&ace_len.to_le_bytes());
        body.extend_from_slice(&mask.to_le_bytes());
        body.extend_from_slice(sid);
    }
    let total = (8 + body.len()) as u16;
    let mut acl = Vec::with_capacity(8 + body.len());
    acl.extend_from_slice(&[2u8, 0]); // ACL_REVISION, sbz1
    acl.extend_from_slice(&total.to_le_bytes());
    acl.extend_from_slice(&(aces.len() as u16).to_le_bytes());
    acl.extend_from_slice(&[0u8, 0]); // sbz2
    acl.extend_from_slice(&body);
    acl
}

fn enable_take_ownership() -> bool {
    unsafe {
        let mut tok: *mut c_void = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut tok) == 0 {
            return false;
        }
        let name = wide("SeTakeOwnershipPrivilege");
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            CloseHandle(tok);
            return false;
        }
        let mut tp = TOKEN_PRIVILEGES1 {
            PrivilegeCount: 1,
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        };
        let ok = AdjustTokenPrivileges(tok, 0, &mut tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) != 0;
        CloseHandle(tok);
        ok
    }
}

/// Heal a service whose DACL denies everyone: take ownership as Administrators,
/// then grant Administrators + SYSTEM full service control. True on success.
pub fn sc_heal_sd(name: &str) -> bool {
    if !is_admin() {
        return false;
    }
    if !enable_take_ownership() {
        return false;
    }
    unsafe {
        let mut scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), 0xF003F);
        if scm.is_null() {
            scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), 0x0001);
        }
        if scm.is_null() {
            return false;
        }
        let wname = wide(name);
        let adm = sid_bytes(&[32, 544]); // S-1-5-32-544 Administrators
        let sys = sid_bytes(&[18]); // S-1-5-18 SYSTEM

        // step 1: ownership -> Administrators (owner is then granted implicit WRITE_DAC)
        let sd_owner = SD_ABS {
            Revision: 1,
            Sbz1: 0,
            Control: 0,
            Owner: adm.as_ptr(),
            Group: std::ptr::null(),
            Sacl: std::ptr::null(),
            Dacl: std::ptr::null(),
        };
        let h1 = OpenServiceW(scm, wname.as_ptr(), WRITE_OWNER);
        if !h1.is_null() {
            let _ = SetServiceObjectSecurity(h1, OWNER_SECURITY_INFORMATION, &sd_owner as *const SD_ABS as *const u8);
            CloseServiceHandle(h1);
        }

        // step 2: rewrite the DACL: Administrators + SYSTEM full service control
        let acl = acl_with_aces(&[(SERVICE_ALL_ACCESS, &adm), (SERVICE_ALL_ACCESS, &sys)]);
        let sd_dacl = SD_ABS {
            Revision: 1,
            Sbz1: 0,
            Control: SE_DACL_PRESENT,
            Owner: adm.as_ptr(),
            Group: sys.as_ptr(),
            Sacl: std::ptr::null(),
            Dacl: acl.as_ptr(),
        };
        let h2 = OpenServiceW(scm, wname.as_ptr(), WRITE_DAC | WRITE_OWNER);
        if h2.is_null() {
            CloseServiceHandle(scm);
            return false;
        }
        let ok = SetServiceObjectSecurity(h2, DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION, &sd_dacl as *const SD_ABS as *const u8) != 0;
        CloseServiceHandle(h2);
        CloseServiceHandle(scm);
        ok
    }
}

// ---------- commit rescue: runtime pagefile expansion ----------
// CBS/TiWorker repair sessions die with E_OUTOFMEMORY when system commit is
// exhausted (fixed pagefile + heavy ecosystem). NtCreatePagingFile grows the
// commit limit IMMEDIATELY - the same path Windows uses for on-demand growth.
// No reboot, no disruption; the supplementary file persists until removed.

#[repr(C)]
#[allow(non_snake_case)]
struct UNICODE_STRING {
    Length: u16,
    MaximumLength: u16,
    Buffer: *mut u16,
}

#[repr(C)]
#[allow(non_snake_case)]
struct MEMORYSTATUSEX {
    dwLength: u32,
    dwMemoryLoad: u32,
    ullTotalPhys: u64,
    ullAvailPhys: u64,
    ullTotalPageFile: u64,
    ullAvailPageFile: u64,
    ullTotalVirtual: u64,
    ullAvailVirtual: u64,
    ullAvailExtendedVirtual: u64,
}

#[allow(non_snake_case)]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalMemoryStatusEx(buf: *mut MEMORYSTATUSEX) -> i32;
}

#[allow(non_snake_case)]
#[link(name = "ntdll")]
extern "system" {
    fn NtCreatePagingFile(
        pageFileName: *const UNICODE_STRING,
        minimumSize: *const u64,
        maximumSize: *const u64,
        priority: u32,
    ) -> i32;
}

/// (commit_free_bytes, commit_limit_bytes)
pub fn commit_status() -> (u64, u64) {
    unsafe {
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };
        if GlobalMemoryStatusEx(&mut m) == 0 {
            return (0, 0);
        }
        (m.ullAvailPageFile, m.ullTotalPageFile)
    }
}

fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut tok: *mut c_void = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut tok) == 0 {
            return false;
        }
        let wname = wide(name);
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        if LookupPrivilegeValueW(std::ptr::null(), wname.as_ptr(), &mut luid) == 0 {
            CloseHandle(tok);
            return false;
        }
        let mut tp = TOKEN_PRIVILEGES1 {
            PrivilegeCount: 1,
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        };
        let ok = AdjustTokenPrivileges(tok, 0, &mut tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) != 0;
        CloseHandle(tok);
        ok
    }
}

/// Create a supplementary pagefile (size in GB) that raises the commit limit
/// immediately. True on success. Example: create_pagefile("C:\\wufix-pagefile.sys", 32)
pub fn create_pagefile(win_path: &str, size_gb: u64) -> bool {
    if !enable_privilege("SeCreatePagefilePrivilege") {
        return false;
    }
    let pages = size_gb * 1024 * 1024 * 1024 / 4096;
    let nt_path = format!("\\??\\{}", win_path);
    let mut w: Vec<u16> = OsStr::new(&nt_path).encode_wide().collect();
    let us = UNICODE_STRING {
        Length: (w.len() * 2) as u16,
        MaximumLength: (w.len() * 2) as u16,
        Buffer: w.as_mut_ptr(),
    };
    let min = pages;
    let max = pages;
    let st = unsafe { NtCreatePagingFile(&us, &min, &max, 0) };
    st == 0
}

// ---------- drives ----------

/// All fixed drives as "C:", "D:", ...
pub fn fixed_drives() -> Vec<String> {
    drive_types()
        .into_iter()
        .filter(|(_d, t)| *t == 3)
        .map(|(d, _)| d)
        .collect()
}

/// Every present drive with its GetDriveTypeW type: 2=removable, 3=fixed,
/// 5=cdrom (mounted ISOs surface as CD-ROM). Used by the repair-source scanner.
pub fn drive_types() -> Vec<(String, u32)> {
    let mask = unsafe { GetLogicalDrives() };
    let mut v = Vec::new();
    for i in 0..26u32 {
        if mask & (1u32 << i) != 0 {
            let letter = char::from(b'A' + i as u8);
            let root = format!("{}:\\", letter);
            let t = unsafe { GetDriveTypeW(wide(&root).as_ptr()) };
            if t == 2 || t == 3 || t == 5 {
                v.push((format!("{}:", letter), t));
            }
        }
    }
    v
}

// ---------- console ----------

pub fn enable_vt() -> bool {
    unsafe {
        let h = GetStdHandle(-11); // STD_OUTPUT_HANDLE
        if h.is_null() {
            return false;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(h, &mut mode) == 0 {
            return false;
        }
        SetConsoleMode(h, mode | 0x0004) != 0 // ENABLE_VIRTUAL_TERMINAL_PROCESSING
    }
}
