//! Process and listening-port inspection for the Info panel's PROCESSES and
//! PORTS sections: the processes belonging to a session (tracked via Windows
//! Job Objects, falling back to the shell's ppid-descendant tree) plus the
//! TCP ports they listen on. Ports additionally include any process working
//! in the project directory, so dev servers launched outside the session
//! (another terminal, an earlier app run) still show up — except for overly
//! broad roots (home dir, drive root), where directory matching is pure noise
//! and only session-owned ports are shown.

use std::collections::{HashMap, HashSet, VecDeque};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, System};
use uuid::Uuid;

/// One row of the PROCESSES section.
#[derive(Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem_bytes: u64,
    pub exe: String,
}

/// One row of the PORTS section: a listening TCP port owned by one of the
/// session's processes or by a process working in the project directory.
#[derive(Debug, Serialize)]
pub struct ListenPort {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
}

/// Cap on PROCESSES rows so a runaway child tree can't flood the panel.
const MAX_PROCESSES: usize = 50;

/// Build a `Command` for a console tool without popping a console window
/// when the GUI app spawns it. On non-Windows this is a plain passthrough.
pub fn quiet_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Shared `System` so per-process CPU% is computed against the previous poll
/// (sysinfo needs two refreshes to produce a delta; polls run ~2s apart, well
/// above its minimum CPU update interval).
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new()));

/// One-shot refresh of the shared sysinfo cache. Call once per poll cycle
/// (or per command invocation) and then use the `_with_sys` variants; CPU%
/// is measured against the previous call of this, so the best cadence is
/// "once per ~3s poll", not once per session. Holding the SYSTEM lock while
/// iterating ~thousands of processes costs ~10–40ms; calling it N times per
/// poll (once per agent session) was the bottleneck that prompted this split.
pub fn refresh_global() {
    let mut sys = SYSTEM.lock();
    sys.refresh_processes_specifics(ProcessRefreshKind::everything());
}

/// Lock the shared `System` cache, refresh it, and return the guard. One
/// acquisition serves the whole batch - pass `&*guard` to the `_with_sys`
/// variants so they don't re-lock (parking_lot's Mutex is NOT reentrant,
/// so a guard held here plus a `SYSTEM.lock()` inside a callee would deadlock).
pub fn refresh_and_snapshot() -> parking_lot::MutexGuard<'static, System> {
    let mut sys = SYSTEM.lock();
    sys.refresh_processes_specifics(ProcessRefreshKind::everything());
    sys
}

/// The pids belonging to a session: every process in its Windows Job Object
/// when one is registered (see `jobs` below), otherwise the shell plus its
/// ppid descendants as a fallback. The shell pid comes first when present,
/// so the panel can keep it as the top row.
///
/// Why jobs: MSMS2/Git-Bash (used by tools like opencode) re-parents children
/// through a short-lived stub, so grandchildren's ppid chains never reach the
/// shell and the BFS silently loses them (e.g. `npm run dev`'s node.exe).
/// Job membership, by contrast, is inherited at process creation regardless
/// of the ppid chain.
///
/// This convenience entry refreshes the shared cache first, so each call is
/// self-contained; for batch callers (the agent poller iterating several
/// sessions), call `refresh_global()` once and use `session_pids_with_sys`
/// to avoid an N-times-per-poll refresh.
pub fn session_pids(session_id: Uuid, fallback_shell_pid: u32) -> Vec<u32> {
    refresh_global();
    let sys = SYSTEM.lock();
    session_pids_with_sys(&sys, session_id, fallback_shell_pid)
}

/// Same as `session_pids` but reads an already-refreshed `System` cache
/// instead of refreshing one itself. The intended call shape for a batch
/// poller: `refresh_global()` once, then `session_pids_with_sys` for each
/// session — saves N−1 full-system refreshes per poll.
pub fn session_pids_with_sys(sys: &System, session_id: Uuid, fallback_shell_pid: u32) -> Vec<u32> {
    if let Some(mut pids) = jobs::query_pids(session_id) {
        // Exited processes can linger in the job's id list briefly; drop them.
        pids.retain(|&pid| sys.process(Pid::from_u32(pid)).is_some());
        if !pids.is_empty() {
            pids.sort_unstable();
            if let Some(i) = pids.iter().position(|&p| p == fallback_shell_pid) {
                pids.swap(0, i);
            }
            return pids;
        }
    }
    descendant_pids(sys, fallback_shell_pid)
}

/// Shape pid rows for the PROCESSES section (name/cpu/mem/exe), preserving
/// the order `session_pids` produced (shell first). Reads the cache filled
/// by `session_pids` — call them back to back.
pub fn process_infos(pids: &[u32]) -> Vec<ProcessInfo> {
    let sys = SYSTEM.lock();
    pids.iter()
        .take(MAX_PROCESSES)
        .filter_map(|&pid| {
            let p = sys.process(Pid::from_u32(pid))?;
            // Skip stale/zombie entries with no image name.
            let name = p.name().to_string();
            if name.is_empty() {
                return None;
            }
            Some(ProcessInfo {
                pid,
                name,
                cpu: p.cpu_usage(),
                mem_bytes: p.memory(),
                exe: p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

/// Image name + full command line of one process, from the shared sysinfo
/// cache (so it must follow a `session_pids` call in the same poll). Used by
/// agent detection to recognise node-based CLI launchers whose image name is
/// just `node.exe`.
pub fn process_cmdline(pid: u32) -> Option<(String, String)> {
    let sys = SYSTEM.lock();
    process_cmdline_with_sys(&sys, pid)
}

/// Same as `process_cmdline` but reads a caller-provided `System` cache.
/// Use this inside a batch that already holds a `refresh_and_snapshot()`
/// guard - `process_cmdline` would re-lock the `SYSTEM` mutex and deadlock
/// against the held guard (parking_lot Mutex is not reentrant).
pub fn process_cmdline_with_sys(sys: &System, pid: u32) -> Option<(String, String)> {
    let p = sys.process(Pid::from_u32(pid))?;
    Some((p.name().to_string(), p.cmd().join(" ")))
}

/// The shell itself plus every descendant (BFS over parent/child links), in
/// display order: shell first, then direct children, then deeper descendants;
/// by pid within each tier.
///
/// Windows caveat: parent PIDs only chain back to the shell while every
/// intermediate process is still alive — if a middle process exits, its
/// children keep running but their `parent()` chain no longer reaches the
/// shell, so they silently drop out of the list. Used only as the fallback
/// when a session has no Job Object.
fn descendant_pids(sys: &System, shell_pid: u32) -> Vec<u32> {
    // parent -> children index for the BFS.
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, proc_) in sys.processes() {
        if let Some(parent) = proc_.parent() {
            children.entry(parent.as_u32()).or_default().push(pid.as_u32());
        }
    }

    // BFS from the shell, remembering each pid's depth.
    let root = Pid::from_u32(shell_pid);
    let mut order: Vec<(u32, u32)> = Vec::new(); // (pid, depth)
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    if sys.process(root).is_some() {
        order.push((shell_pid, 0));
        queue.push_back((shell_pid, 0));
    }
    while let Some((pid, depth)) = queue.pop_front() {
        let mut kids = children.get(&pid).cloned().unwrap_or_default();
        kids.sort_unstable();
        for kid in kids {
            order.push((kid, depth + 1));
            queue.push_back((kid, depth + 1));
        }
    }
    order.sort_by_key(|&(pid, depth)| (depth.min(2), pid));
    order.into_iter().map(|(pid, _)| pid).collect()
}

/// Per-session Windows Job Objects. Each tracked session's shell is assigned
/// to a fresh job; every process the tree spawns joins it automatically, so
/// `session_pids` sees the whole tree no matter how ppid links break.
#[cfg(windows)]
mod jobs {
    use super::{Mutex, Uuid};
    use std::collections::HashMap;

    use once_cell::sync::Lazy;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, QueryInformationJobObject,
        SetInformationJobObject, JOBOBJECT_BASIC_PROCESS_ID_LIST,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// HANDLE isn't Send; we only ever move it between threads behind the
    /// mutex, which is sound for a job handle.
    struct SendHandle(usize);
    unsafe impl Send for SendHandle {}

    impl SendHandle {
        fn handle(&self) -> HANDLE {
            HANDLE(self.0 as *mut std::ffi::c_void)
        }
    }

    static JOBS: Lazy<Mutex<HashMap<Uuid, SendHandle>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    /// Create a kill-on-close job and assign `child_pid` (the session's
    /// freshly spawned shell) to it. Best-effort: any failure is logged and
    /// leaves the session untracked.
    pub fn track(session_id: Uuid, child_pid: u32) {
        unsafe {
            let job = match CreateJobObjectW(None, windows::core::PCWSTR::null()) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("track_session {session_id}: CreateJobObjectW failed: {e}");
                    return;
                }
            };
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .is_err()
            {
                let _ = CloseHandle(job);
                log::warn!("track_session {session_id}: SetInformationJobObject failed");
                return;
            }
            let proc = match OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child_pid) {
                Ok(p) => p,
                Err(e) => {
                    let _ = CloseHandle(job);
                    log::warn!("track_session {session_id}: OpenProcess({child_pid}) failed: {e}");
                    return;
                }
            };
            let assigned = AssignProcessToJobObject(job, proc);
            let _ = CloseHandle(proc);
            if assigned.is_err() {
                // A process can belong to only one job (nested jobs need
                // Win8+ semantics that tooling like VS/CI often breaks), so
                // assignment legitimately fails for shells already inside a
                // job. Drop ours and record nothing — the ppid-BFS fallback
                // in session_pids still covers this session.
                let _ = CloseHandle(job);
                log::warn!("track_session {session_id}: AssignProcessToJobObject failed; using BFS fallback");
                return;
            }
            if let Some(old) = JOBS.lock().insert(session_id, SendHandle(job.0 as usize)) {
                let _ = CloseHandle(old.handle());
            }
        }
    }

    /// Pids currently in the session's job, or None when the session isn't
    /// tracked (caller falls back to the ppid BFS).
    pub fn query_pids(session_id: Uuid) -> Option<Vec<u32>> {
        let jobs = JOBS.lock();
        let job = jobs.get(&session_id)?.handle();
        // Two-call pattern: query into a small buffer; if the job grew more
        // processes than it fits, resize to NumberOfProcessIdsInList and
        // re-query. Buffer is usize-words so the struct stays aligned.
        let mut cap: usize = 16;
        unsafe {
            loop {
                let header_words =
                    std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() / std::mem::size_of::<usize>();
                let mut buf = vec![0usize; header_words + cap.saturating_sub(1)];
                if QueryInformationJobObject(
                    job,
                    JobObjectBasicProcessIdList,
                    buf.as_mut_ptr() as *mut _,
                    (buf.len() * std::mem::size_of::<usize>()) as u32,
                    None,
                )
                .is_err()
                {
                    return Some(Vec::new());
                }
                let list = &*(buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST);
                let count = list.NumberOfProcessIdsInList as usize;
                if count > cap {
                    cap = count;
                    continue;
                }
                let ids = std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), count);
                return Some(ids.iter().map(|&p| p as u32).collect());
            }
        }
    }

    /// Close the session's job handle. Because of KILL_ON_JOB_CLOSE this also
    /// terminates every process still in the job — intended: closing a
    /// session cleans up its dev servers and other orphaned grandchildren
    /// that would otherwise outlive the shell.
    pub fn untrack(session_id: Uuid) {
        if let Some(h) = JOBS.lock().remove(&session_id) {
            unsafe {
                let _ = CloseHandle(h.handle());
            }
        }
    }
}

#[cfg(not(windows))]
mod jobs {
    use super::Uuid;
    pub fn track(_session_id: Uuid, _child_pid: u32) {}
    pub fn query_pids(_session_id: Uuid) -> Option<Vec<u32>> { None }
    pub fn untrack(_session_id: Uuid) {}
}

/// Register the session's shell pid under a kill-on-close Job Object (see
/// the `jobs` module). Best-effort; failures fall back to ppid BFS.
pub fn track_session(session_id: Uuid, child_pid: u32) {
    jobs::track(session_id, child_pid);
}

/// Stop tracking a session, closing its job (which also kills everything
/// still in the job — see `jobs::untrack`).
pub fn untrack_session(session_id: Uuid) {
    jobs::untrack(session_id);
}

/// Force-kill a process. Windows has no SIGTERM/SIGKILL distinction — this is
/// TerminateProcess semantics only — so there is a single kill command and
/// the frontend labels it "Kill process".
pub fn kill(pid: u32) -> Result<(), String> {
    let mut sys = SYSTEM.lock();
    sys.refresh_pids_specifics(&[Pid::from_u32(pid)], ProcessRefreshKind::new());
    let Some(proc_) = sys.process(Pid::from_u32(pid)) else {
        return Err(format!("no process with pid {pid}"));
    };
    if proc_.kill() {
        Ok(())
    } else {
        Err(format!("failed to kill pid {pid}"))
    }
}

/// Listening TCP ports via `netstat -ano -p tcp` (available on every Windows
/// version), owned by one of `pids` (the session's processes). When
/// `project_root` is Some (the "project ports" setting), listeners owned by
/// any process belonging to that directory — its working directory is the
/// root or below, or its command line references the root — are included too,
/// so a dev server started outside the session still shows up. The directory
/// fallback is skipped for overly broad roots (home directory, drive roots):
/// there it matches unrelated system processes, which is noise rather than
/// signal.
///
/// Parse failures are tolerated: rows that don't parse are skipped and
/// whatever parsed is returned.
pub fn listen_ports(pids: &[u32], project_root: Option<&str>) -> Vec<ListenPort> {
    let wanted: HashSet<u32> = pids.iter().copied().collect();
    let root = project_root
        .map(normalize_path)
        .filter(|r| !r.is_empty() && !too_broad_root(r));
    let mut cmd = quiet_command("netstat");
    cmd.args(["-ano", "-p", "tcp"]);
    let Ok(output) = cmd.output() else { return Vec::new() };
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut seen: HashSet<u16> = HashSet::new();
    let mut rows: Vec<(u16, u32)> = Vec::new();
    let mut cwd_cache: HashMap<u32, Option<String>> = HashMap::new();
    for line in stdout.lines() {
        let Some((port, pid)) = parse_netstat_line(line) else { continue };
        if !wanted.contains(&pid) {
            let Some(root) = &root else { continue };
            if !belongs_to_project(pid, root, &mut cwd_cache) {
                continue;
            }
        }
        // Same port may appear twice (IPv4 + IPv6) — keep the first.
        if !seen.insert(port) {
            continue;
        }
        rows.push((port, pid));
    }

    let matched: Vec<u32> = rows.iter().map(|&(_, pid)| pid).collect();
    let names = process_names(&matched);
    let mut ports: Vec<ListenPort> = rows
        .into_iter()
        .map(|(port, pid)| ListenPort {
            port,
            pid,
            process_name: names.get(&pid).cloned().unwrap_or_default(),
        })
        .collect();
    ports.sort_by_key(|p| p.port);
    ports
}

/// Lowercase with forward slashes converted to backslashes and trailing
/// separators stripped, so project paths compare equal no matter how the
/// user (or a tool like Git Bash) spelled them.
fn normalize_path(p: &str) -> String {
    let n = p.replace('/', "\\").to_lowercase();
    let t = n.trim_end_matches('\\');
    // Don't strip a drive root: "c:" would prefix-match the whole drive.
    if t.len() == 2 && t.ends_with(':') {
        return format!("{t}\\");
    }
    t.to_string()
}

/// Does `hay` contain the path `root` on a path boundary? Guards against
/// "d:\projects\foo2" matching root "d:\projects\foo". Both args must be
/// normalize_path'd already.
fn contains_path(hay: &str, root: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(root) {
        let end = from + i + root.len();
        match hay[end..].chars().next() {
            None => return true,
            Some('\\' | '"' | '\'' | ' ' | ';' | '&' | '|') => return true,
            _ => from = end,
        }
    }
    false
}

/// Is `dir` the project root or a directory below it? Both normalize_path'd.
fn dir_within(dir: &str, root: &str) -> bool {
    dir == root || dir.starts_with(&format!("{root}\\"))
}

/// Is `root` (normalize_path'd) too broad for directory-based port matching?
/// A "project" rooted at the user's home directory or at a drive root sweeps
/// in every process that happens to run from there (proxy tools, Docker,
/// launchers) — pure noise — so the directory fallback is skipped for those
/// and only session-owned ports are shown.
fn too_broad_root(root: &str) -> bool {
    let components = root.split('\\').filter(|s| !s.is_empty()).count();
    if components < 2 {
        return true; // drive root like "d:\"
    }
    if let Some(home) = dirs::home_dir() {
        if root == normalize_path(&home.to_string_lossy()) {
            return true;
        }
    }
    false
}

/// Does the process belong to the project at `root` (normalize_path'd)?
/// Cheap check first — the full command line (from the shared sysinfo cache,
/// already refreshed by session_pids this poll) mentioning the root catches
/// node/npm-style servers whose node_modules paths sit inside the project.
/// Otherwise read the process's actual working directory from its PEB, which
/// also catches servers started with relative paths (e.g. `python -m
/// http.server` run inside the project). `cwd_cache` dedupes the PEB read
/// across ports owned by the same pid within one poll.
fn belongs_to_project(pid: u32, root: &str, cwd_cache: &mut HashMap<u32, Option<String>>) -> bool {
    {
        let sys = SYSTEM.lock();
        if let Some(p) = sys.process(Pid::from_u32(pid)) {
            let cmd = p.cmd().join(" ");
            if contains_path(&normalize_path(&cmd), root) {
                return true;
            }
        }
    }
    let cwd = cwd_cache
        .entry(pid)
        .or_insert_with(|| process_cwd(pid).map(|c| normalize_path(&c)));
    cwd.as_deref().is_some_and(|c| dir_within(c, root))
}

/// The process's current working directory, read from its PEB
/// (ProcessParameters->CurrentDirectory). Best-effort: None for
/// access-denied, exited, or 32-bit processes (their PEB layout differs).
/// Uses the stable x64/ARM64 field offsets; nothing here is written, only
/// read, and every read is bounds-checked by the OS.
#[cfg(windows)]
fn process_cwd(pid: u32) -> Option<String> {
    use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_VM_READ,
    };

    /// PEB->ProcessParameters.
    const PEB_PROCESS_PARAMETERS: usize = 0x20;
    /// RTL_USER_PROCESS_PARAMETERS->CurrentDirectory.DosPath, a
    /// UNICODE_STRING { u16 len; u16 max; pad; *mut u16 buf }.
    const CURDIR_DOS_PATH: usize = 0x38;
    const MAX_CWD_BYTES: u16 = 4096;

    unsafe {
        let proc =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
        let result = (|| {
            let read = |addr: usize, buf: *mut core::ffi::c_void, len: usize| {
                ReadProcessMemory(proc, addr as *const _, buf, len, None).ok()
            };
            let read_usize = |addr: usize| -> Option<usize> {
                let mut v = 0usize;
                read(addr, &mut v as *mut _ as *mut _, std::mem::size_of::<usize>())?;
                Some(v)
            };

            let mut info = PROCESS_BASIC_INFORMATION::default();
            NtQueryInformationProcess(
                proc,
                ProcessBasicInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
            .is_ok()
            .then_some(())?;

            let peb = info.PebBaseAddress as usize;
            let params = read_usize(peb + PEB_PROCESS_PARAMETERS)?;
            let len = {
                let mut v = 0u16;
                read(params + CURDIR_DOS_PATH, &mut v as *mut _ as *mut _, 2)?;
                v
            };
            if len == 0 || len > MAX_CWD_BYTES {
                return None;
            }
            let buf = read_usize(params + CURDIR_DOS_PATH + 8)?;
            if buf == 0 {
                return None;
            }
            let mut wide = vec![0u16; (len / 2) as usize];
            read(buf, wide.as_mut_ptr() as *mut _, len as usize)?;
            Some(String::from_utf16_lossy(&wide))
        })();
        let _ = CloseHandle(proc);
        result
    }
}

#[cfg(not(windows))]
fn process_cwd(_pid: u32) -> Option<String> {
    None
}

/// pid -> image name lookup from the shared `System` without refreshing: the
/// frontend polls PROCESSES right before PORTS, so the cache is fresh.
fn process_names(pids: &[u32]) -> HashMap<u32, String> {
    let sys = SYSTEM.lock();
    pids.iter()
        .filter_map(|&pid| {
            sys.process(Pid::from_u32(pid))
                .map(|p| (pid, p.name().to_string()))
        })
        .collect()
}

/// Parse one `netstat -ano -p tcp` row into (local port, pid), or None for
/// headers, non-LISTENING rows, and malformed lines. Row shape:
/// `  TCP    127.0.0.1:3000         0.0.0.0:0              LISTENING       12345`
fn parse_netstat_line(line: &str) -> Option<(u16, u32)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 5 || cols[0] != "TCP" || cols[3] != "LISTENING" {
        return None;
    }
    let port: u16 = cols[1].rsplit(':').next()?.parse().ok()?;
    let pid: u32 = cols[4].parse().ok()?;
    Some((port, pid))
}

#[cfg(test)]
mod tests {
    use super::{contains_path, dir_within, normalize_path, parse_netstat_line};

    /// Roundtrip of the Job Object tracking: spawn a sleeper, track it, see
    /// it in session_pids, then untrack and verify KILL_ON_JOB_CLOSE reaped
    /// it. Spawns and kills a real process, so it's excluded from the default
    /// run — verify manually with `cargo test --lib -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "spawns/kills a real process; run manually: cargo test --lib -- --ignored"]
    fn job_tracks_pids_and_kills_on_untrack() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ping");
        let pid = child.id();

        let sid = uuid::Uuid::new_v4();
        super::track_session(sid, pid);
        let pids = super::session_pids(sid, pid);
        assert!(pids.contains(&pid), "tracked pid missing from {pids:?}");
        assert_eq!(pids.first(), Some(&pid), "shell pid should come first");

        super::untrack_session(sid);
        // KILL_ON_JOB_CLOSE fires on the close; give the OS up to 2s to reap.
        let deadline = Instant::now() + Duration::from_secs(2);
        let alive = loop {
            match child.try_wait() {
                Ok(Some(_)) => break false,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(None) => break true,
                Err(_) => break false,
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        assert!(!alive, "process survived job close");
    }

    #[test]
    fn parses_ipv4_listening_row() {
        let line = "  TCP    127.0.0.1:3000         0.0.0.0:0              LISTENING       12345";
        assert_eq!(parse_netstat_line(line), Some((3000, 12345)));
    }

    #[test]
    fn parses_ipv6_listening_row() {
        let line = "  TCP    [::1]:8080               [::]:0                 LISTENING       4321";
        assert_eq!(parse_netstat_line(line), Some((8080, 4321)));
    }

    #[test]
    fn parses_wildcard_and_udp_is_skipped() {
        let tcp = "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       980";
        assert_eq!(parse_netstat_line(tcp), Some((135, 980)));
        let udp = "  UDP    0.0.0.0:5353           *:*                                    980";
        assert_eq!(parse_netstat_line(udp), None);
    }

    #[test]
    fn skips_non_listening_and_garbage() {
        let established = "  TCP    10.0.0.1:5000          10.0.0.2:80            ESTABLISHED     99";
        assert_eq!(parse_netstat_line(established), None);
        assert_eq!(parse_netstat_line("  Proto  Local Address          Foreign Address        State           PID"), None);
        assert_eq!(parse_netstat_line(""), None);
        assert_eq!(parse_netstat_line("  TCP    127.0.0.1:abc          0.0.0.0:0              LISTENING       x"), None);
    }

    #[test]
    fn normalize_path_cases() {
        assert_eq!(normalize_path("D:/Agents/XS-Studio/"), "d:\\agents\\xs-studio");
        assert_eq!(normalize_path("d:\\agents\\xs-studio\\\\"), "d:\\agents\\xs-studio");
        // A drive root keeps its separator so prefix checks can't match the
        // whole drive.
        assert_eq!(normalize_path("C:\\"), "c:\\");
        assert_eq!(normalize_path("c:/"), "c:\\");
    }

    #[test]
    fn contains_path_needs_boundary() {
        let root = "d:\\agents\\xs-studio";
        assert!(contains_path("node d:\\agents\\xs-studio\\node_modules\\next\\x.js", root));
        assert!(contains_path("\"d:\\agents\\xs-studio\" extra", root));
        assert!(contains_path("cd d:\\agents\\xs-studio", root));
        assert!(!contains_path("node d:\\agents\\xs-studio2\\server.js", root));
        assert!(!contains_path("node d:\\other\\server.js", root));
    }

    #[test]
    fn dir_within_cases() {
        let root = "d:\\agents\\xs-studio";
        assert!(dir_within(root, root));
        assert!(dir_within("d:\\agents\\xs-studio\\sub\\dir", root));
        assert!(!dir_within("d:\\agents\\xs-studio2", root));
        assert!(!dir_within("d:\\agents", root));
    }

    #[test]
    fn too_broad_root_cases() {
        assert!(super::too_broad_root("d:\\"), "drive root is broad");
        assert!(super::too_broad_root("c:\\"), "drive root is broad");
        assert!(!super::too_broad_root("d:\\agents"));
        assert!(!super::too_broad_root("d:\\agents\\xs-studio"));
        let home = dirs::home_dir().expect("home dir");
        let root = super::normalize_path(&home.to_string_lossy());
        assert!(super::too_broad_root(&root), "home directory is broad");
    }

    /// Live check of the PEB cwd read: spawn a sleeper with a known working
    /// directory and verify process_cwd reads it back and belongs_to_project
    /// matches it; then an end-to-end listen_ports pass over a real listener
    /// owned by this test process. Spawns real processes and netstat — verify
    /// manually with `cargo test --lib -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "spawns real processes; run manually: cargo test --lib -- --ignored"]
    fn reads_process_cwd_and_matches_project() {
        use std::process::{Command, Stdio};

        let dir = std::env::temp_dir().join("muster-cwd-test");
        std::fs::create_dir_all(&dir).unwrap();
        let root = super::normalize_path(&dir.to_string_lossy());
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ping");
        let pid = child.id();

        let cwd = super::process_cwd(pid).expect("cwd read failed");
        assert_eq!(super::normalize_path(&cwd), root);
        // The cmdline ("ping -n 30 ...") doesn't mention the dir, so this
        // exercises the cwd path specifically.
        let mut cache = std::collections::HashMap::new();
        assert!(super::belongs_to_project(pid, &root, &mut cache));

        let _ = child.kill();
        let _ = child.wait();
    }

    /// End-to-end: a listener owned by this test process shows up in
    /// listen_ports when its pid is passed. Runs netstat — verify manually
    /// with `cargo test --lib -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "runs netstat; run manually: cargo test --lib -- --ignored"]
    fn finds_own_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mine = super::listen_ports(&[std::process::id()], None);
        assert!(
            mine.iter().any(|p| p.port == port),
            "own listener on {port} missing from {mine:?}"
        );
    }
}
