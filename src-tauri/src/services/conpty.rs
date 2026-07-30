//! Manual ConPTY driver using a bundled OpenConsole.exe (from the Windows
//! Terminal project, MIT-licensed) instead of the inbox system conhost.exe.
//!
//! The inbox conhost.exe ships with Windows and is only updated via OS
//! cumulative updates. It contains bugs fixed in the Windows Terminal
//! team's newer OpenConsole.exe builds (e.g. the integer-divide-by-zero
//! crash triggered by opencode's exit on Windows 11 25H2 build 26200).
//! By bundling OpenConsole.exe and driving it manually we get a console
//! host identical to what Windows Terminal uses, decoupling Muster from
//! OS-version-specific conhost bugs.
//!
//! Mirrors microsoft/terminal's winconpty library (src/winconpty/
//! winconpty.cpp + device.cpp).

#![cfg(windows)]

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle};
use std::ptr;

use once_cell::sync::OnceCell;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{BOOL, HANDLE, HANDLE_FLAGS, CloseHandle, SetHandleInformation};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::WriteFile;
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::*;

// ── NT definitions ─────────────────────────────────────────────────────

#[repr(C)]
#[derive(Default)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    security_quality_of_service: *const c_void,
}

#[repr(C)]
#[derive(Default)]
struct IoStatusBlock {
    status: i32,
    information: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtOpenFile(
        file_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *const ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        share_access: u32,
        open_options: u32,
    ) -> i32;

    fn NtSetSystemInformation(
        system_information_class: u32,
        system_information: *mut c_void,
        system_information_length: u32,
    ) -> i32;
}

// ── Constants ──────────────────────────────────────────────────────────

const OBJ_CASE_INSENSITIVE: u32 = 0x00000040;
const OBJ_INHERIT: u32 = 0x00000002;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x00000020;
const FILE_SHARE_ALL: u32 = 0x00000007;
const GENERIC_ALL: u32 = 0x10000000;
const GENERIC_READ: u32 = 0x80000000;
const GENERIC_WRITE: u32 = 0x40000000;
const SYNCHRONIZE: u32 = 0x00100000;
const HANDLE_FLAG_INHERIT: u32 = 0x00000001;
const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x00020002;
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;
const PTY_SIGNAL_RESIZE_WINDOW: u16 = 8;
const STARTF_USESTDHANDLES: u32 = 0x00000100;
const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x00080000;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x00000400;
const STILL_ACTIVE: u32 = 259;
const SYSTEM_CONSOLE_INFORMATION_CLASS: u32 = 132;

#[repr(C)]
struct PseudoConsole {
    h_signal: HANDLE,
    h_pty_reference: HANDLE,
    h_conpty_process: HANDLE,
}

unsafe impl Send for PseudoConsole {}
unsafe impl Sync for PseudoConsole {}

// ── OpenConsole.exe path ───────────────────────────────────────────────

static OPENCONSOLE_PATH: OnceCell<String> = OnceCell::new();

pub fn init_openconsole_path(path: String) {
    let _ = OPENCONSOLE_PATH.set(path);
}

fn openconsole_path() -> String {
    OPENCONSOLE_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| "OpenConsole.exe".to_string())
}

// ── ConPty ─────────────────────────────────────────────────────────────

pub struct ConPty {
    pty: Box<PseudoConsole>,
    signal: Option<OwnedHandle>,
    conhost: Option<OwnedHandle>,
    child: Option<OwnedHandle>,
    child_pid: Option<u32>,
    reader: Option<OwnedHandle>,
    writer: Option<OwnedHandle>,
}

unsafe impl Send for ConPty {}
unsafe impl Sync for ConPty {}

impl ConPty {
    pub fn create(cols: u16, rows: u16) -> io::Result<Self> {
        ensure_driver_loaded();

        let server = create_server_handle(true).or_else(|_| {
            ensure_driver_loaded();
            create_server_handle(true)
        })?;
        let reference = create_client_handle(server, "\\Reference", false)?;

        let (sig_read, sig_write) = new_pipe()?;
        let (in_read, in_write) = new_pipe()?;
        let (out_read, out_write) = new_pipe()?;

        mark_inheritable(&sig_read)?;
        mark_inheritable(&in_read)?;
        mark_inheritable(&out_write)?;

        let conhost = spawn_openconsole(
            &openconsole_path(), cols, rows,
            raw(&sig_read), server, raw(&in_read), raw(&out_write),
        )?;

        drop(sig_read);
        drop(in_read);
        drop(out_write);
        let _ = unsafe { CloseHandle(server) };

        let pty = Box::new(PseudoConsole {
            h_signal: raw(&sig_write),
            h_pty_reference: reference,
            h_conpty_process: raw(&conhost),
        });

        Ok(ConPty {
            pty, signal: Some(sig_write), conhost: Some(conhost),
            child: None, child_pid: None,
            reader: Some(out_read), writer: Some(in_write),
        })
    }

    pub fn spawn_shell(
        &mut self, exe: &str, args: &[String], cwd: &str, env: &[(String, String)],
    ) -> io::Result<u32> {
        let env_block = build_env_block(env);
        let mut cmdline = build_commandline(exe, args);
        let cwd_wide: Vec<u16> = format!("{}\0", cwd).encode_utf16().collect();

        let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags = STARTUPINFOW_FLAGS(STARTF_USESTDHANDLES);

        let mut attr_size: usize = 0;
        unsafe {
            let _ = InitializeProcThreadAttributeList(LPPROC_THREAD_ATTRIBUTE_LIST(ptr::null_mut()), 1, 0, &mut attr_size);
        }
        let mut attr_buf = vec![0u8; attr_size];
        let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
        unsafe {
            InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size).map_err(io::Error::from)?;
        }
        let hpcon = &*self.pty as *const PseudoConsole as *const c_void;
        unsafe {
            UpdateProcThreadAttribute(
                attr_list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                Some(hpcon), std::mem::size_of::<usize>(), None, None,
            ).map_err(io::Error::from)?;
        }
        si.lpAttributeList = attr_list;

        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let result = unsafe {
            CreateProcessW(
                PCWSTR::null(),
                PWSTR(cmdline.as_mut_ptr()),
                None, None,
                false,
                PROCESS_CREATION_FLAGS(EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT),
                Some(env_block.as_ptr() as *const c_void),
                PCWSTR::from_raw(cwd_wide.as_ptr()),
                &si.StartupInfo as *const _ as *const STARTUPINFOW,
                &mut pi,
            )
        };
        unsafe { DeleteProcThreadAttributeList(attr_list) };
        result.map_err(io::Error::from)?;

        let _ = unsafe { CloseHandle(pi.hThread) };

        // Release the reference handle (ReleasePseudoConsole semantics).
        unsafe { let _ = CloseHandle(self.pty.h_pty_reference); }
        self.pty.h_pty_reference = HANDLE::default();

        let pid = pi.dwProcessId;
        self.child = Some(unsafe { OwnedHandle::from_raw_handle(pi.hProcess.0) });
        self.child_pid = Some(pid);
        Ok(pid)
    }

    pub fn take_reader(&mut self) -> io::Result<Box<dyn Read + Send>> {
        self.reader.take()
            .map(|h| Box::new(unsafe { std::fs::File::from_raw_handle(h.into_raw_handle()) }) as Box<dyn Read + Send>)
            .ok_or_else(|| io::Error::other("reader already taken"))
    }

    pub fn take_writer(&mut self) -> io::Result<Box<dyn Write + Send>> {
        self.writer.take()
            .map(|h| Box::new(unsafe { std::fs::File::from_raw_handle(h.into_raw_handle()) }) as Box<dyn Write + Send>)
            .ok_or_else(|| io::Error::other("writer already taken"))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let signal = self.signal.as_ref().ok_or_else(|| io::Error::other("signal closed"))?;
        let packet: [u16; 3] = [PTY_SIGNAL_RESIZE_WINDOW, cols, rows];
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &packet as *const _ as *const u8,
                std::mem::size_of_val(&packet),
            )
        };
        let mut written: u32 = 0;
        unsafe {
            WriteFile(
                HANDLE(signal.as_raw_handle()),
                Some(bytes),
                Some(&mut written), None,
            ).map_err(io::Error::from)?;
        }
        Ok(())
    }

    pub fn kill_child(&mut self) {
        if let Some(child) = self.child.as_ref() {
            unsafe { let _ = TerminateProcess(HANDLE(child.as_raw_handle()), 1); }
        }
    }

    pub fn process_id(&self) -> Option<u32> { self.child_pid }

    pub fn is_conhost_alive(&self) -> bool {
        self.conhost.as_ref().is_some_and(|h| {
            let mut code: u32 = 0;
            unsafe { let _ = GetExitCodeProcess(HANDLE(h.as_raw_handle()), &mut code); }
            code == STILL_ACTIVE
        })
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        self.kill_child();
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn raw(h: &OwnedHandle) -> HANDLE { HANDLE(h.as_raw_handle()) }

fn ensure_driver_loaded() {
    let mut info: u32 = 1;
    unsafe {
        NtSetSystemInformation(SYSTEM_CONSOLE_INFORMATION_CLASS, &mut info as *mut _ as *mut c_void, 4);
    }
}

fn create_server_handle(inheritable: bool) -> io::Result<HANDLE> {
    let us = to_unicode_string("\\Device\\ConDrv\\Server");
    let oa = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root_directory: HANDLE::default(),
        object_name: &us,
        attributes: OBJ_CASE_INSENSITIVE | if inheritable { OBJ_INHERIT } else { 0 },
        security_descriptor: ptr::null(),
        security_quality_of_service: ptr::null(),
    };
    let mut handle = HANDLE::default();
    let mut iosb = IoStatusBlock::default();
    let status = unsafe { NtOpenFile(&mut handle, GENERIC_ALL, &oa, &mut iosb, FILE_SHARE_ALL, 0) };
    if status < 0 {
        return Err(io::Error::other(format!("NtOpenFile(Server) NTSTATUS 0x{:08X}", status as u32)));
    }
    Ok(handle)
}

fn create_client_handle(server: HANDLE, name: &str, inheritable: bool) -> io::Result<HANDLE> {
    let us = to_unicode_string(name);
    let oa = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root_directory: server,
        object_name: &us,
        attributes: OBJ_CASE_INSENSITIVE | if inheritable { OBJ_INHERIT } else { 0 },
        security_descriptor: ptr::null(),
        security_quality_of_service: ptr::null(),
    };
    let mut handle = HANDLE::default();
    let mut iosb = IoStatusBlock::default();
    let status = unsafe {
        NtOpenFile(&mut handle, GENERIC_READ | GENERIC_WRITE | SYNCHRONIZE, &oa, &mut iosb, FILE_SHARE_ALL, FILE_SYNCHRONOUS_IO_NONALERT)
    };
    if status < 0 {
        return Err(io::Error::other(format!("NtOpenFile({}) NTSTATUS 0x{:08X}", name, status as u32)));
    }
    Ok(handle)
}

fn to_unicode_string(s: &str) -> UnicodeString {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let len = (wide.len() - 1) * 2;
    UnicodeString { length: len as u16, maximum_length: (len + 2) as u16, buffer: wide.as_ptr() }
}

fn new_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: BOOL(0),
    };
    let mut read_h = HANDLE::default();
    let mut write_h = HANDLE::default();
    unsafe {
        CreatePipe(&mut read_h, &mut write_h, Some(&sa), 0).map_err(io::Error::from)?;
    }
    Ok((
        unsafe { OwnedHandle::from_raw_handle(read_h.0) },
        unsafe { OwnedHandle::from_raw_handle(write_h.0) },
    ))
}

fn mark_inheritable(h: &OwnedHandle) -> io::Result<()> {
    unsafe {
        SetHandleInformation(raw(h), HANDLE_FLAG_INHERIT, HANDLE_FLAGS(HANDLE_FLAG_INHERIT))
            .map_err(io::Error::from)?;
    }
    Ok(())
}

fn spawn_openconsole(
    path: &str, cols: u16, rows: u16,
    signal: HANDLE, server: HANDLE, input: HANDLE, output: HANDLE,
) -> io::Result<OwnedHandle> {
    let cmdline = format!(
        "\"{}\" --headless --width {} --height {} --signal 0x{:x} --server 0x{:x}\0",
        path, cols, rows, signal.0 as usize, server.0 as usize,
    );
    let mut cmdline_wide: Vec<u16> = cmdline.encode_utf16().collect();
    let path_wide: Vec<u16> = format!("{}\0", path).encode_utf16().collect();

    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTUPINFOW_FLAGS(STARTF_USESTDHANDLES);
    si.StartupInfo.hStdInput = input;
    si.StartupInfo.hStdOutput = output;
    si.StartupInfo.hStdError = output;

    let handles = [server, input, output, signal];
    let mut attr_size: usize = 0;
    unsafe {
        let _ = InitializeProcThreadAttributeList(LPPROC_THREAD_ATTRIBUTE_LIST(ptr::null_mut()), 1, 0, &mut attr_size);
    }
    let mut attr_buf = vec![0u8; attr_size];
    let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut _);
    unsafe {
        InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size).map_err(io::Error::from)?;
        UpdateProcThreadAttribute(
            attr_list, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            Some(handles.as_ptr() as *const c_void),
            handles.len() * std::mem::size_of::<HANDLE>(),
            None, None,
        ).map_err(io::Error::from)?;
    }
    si.lpAttributeList = attr_list;

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let result = unsafe {
        CreateProcessW(
            PCWSTR::from_raw(path_wide.as_ptr()),
            PWSTR(cmdline_wide.as_mut_ptr()),
            None, None,
            true,
            PROCESS_CREATION_FLAGS(EXTENDED_STARTUPINFO_PRESENT),
            None,
            PCWSTR::null(),
            &si.StartupInfo as *const _ as *const STARTUPINFOW,
            &mut pi,
        )
    };
    unsafe { DeleteProcThreadAttributeList(attr_list) };
    result.map_err(io::Error::from)?;

    let _ = unsafe { CloseHandle(pi.hThread) };
    Ok(unsafe { OwnedHandle::from_raw_handle(pi.hProcess.0) })
}

fn build_env_block(env: &[(String, String)]) -> Vec<u16> {
    let mut block = Vec::new();
    for (k, v) in env {
        block.extend(format!("{}={}\0", k, v).encode_utf16());
    }
    block.push(0);
    block
}

fn build_commandline(exe: &str, args: &[String]) -> Vec<u16> {
    let mut cmd = quote_arg(exe);
    for arg in args {
        cmd.push(' ');
        cmd.push_str(&quote_arg(arg));
    }
    cmd.push('\0');
    cmd.encode_utf16().collect()
}

fn quote_arg(s: &str) -> String {
    if s.contains(' ') || s.contains('\t') { format!("\"{}\"", s) } else { s.to_string() }
}
