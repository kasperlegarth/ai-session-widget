use crate::model::SessionInfo;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsage {
    /// CPU used by tracked sessions (and their child processes), normalized
    /// to the whole-system scale (0-100), so it's directly comparable to
    /// `total_cpu_percent`.
    pub session_cpu_percent: f32,
    pub total_cpu_percent: f32,
    pub session_memory_bytes: u64,
    pub total_memory_bytes: u64,
    /// Bytes read + written by tracked sessions (and their child processes)
    /// since the previous poll — the frontend divides by its poll interval
    /// to get a rate.
    pub session_disk_bytes: u64,
    /// Bytes read + written by every process on the machine over the same
    /// interval, so the frontend can show what share of it is the tracked
    /// sessions (same shape as `total_cpu_percent`/`total_memory_bytes`).
    pub total_disk_bytes: u64,
    pub orphaned_processes: Vec<OrphanProcess>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanProcess {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub disk_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsPayload {
    pub sessions: Vec<SessionInfo>,
    pub usage: ResourceUsage,
}

#[derive(Debug, Clone)]
pub struct ProcSample {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    /// Per-core-normalized, i.e. 100.0 == one full core saturated
    /// (matches `sysinfo::Process::cpu_usage`).
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    /// Bytes read + written since the previous refresh
    /// (matches `sysinfo::Process::disk_usage`).
    pub disk_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SessionUsageTotals {
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub disk_bytes: u64,
}

/// Sums CPU, memory and disk I/O for each root pid plus all of its
/// descendant processes — a session's CLI process often spawns a subprocess
/// to run a tool (a build, a test run, a shell command), and that subprocess
/// is what actually burns the resource, not the CLI process itself.
///
/// `cpu_percent` on each sample is per-core-normalized; the summed result is
/// divided by `core_count` to bring it onto the same 0-100 scale as
/// `System::global_cpu_usage`.
pub fn sum_session_usage(
    procs: &[ProcSample],
    root_pids: &[u32],
    core_count: usize,
) -> SessionUsageTotals {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        if let Some(parent) = p.parent {
            children.entry(parent).or_default().push(p.pid);
        }
    }
    let by_pid: HashMap<u32, &ProcSample> = procs.iter().map(|p| (p.pid, p)).collect();

    let mut visited = HashSet::new();
    let mut stack: Vec<u32> = root_pids.to_vec();
    let mut totals = SessionUsageTotals::default();

    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some(p) = by_pid.get(&pid) {
            totals.cpu_percent += p.cpu_percent;
            totals.memory_bytes += p.memory_bytes;
            totals.disk_bytes += p.disk_bytes;
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }

    totals.cpu_percent /= core_count.max(1) as f32;
    totals
}

/// Total disk I/O across every process on the machine over the same
/// interval as `sum_session_usage`'s `disk_bytes`, for computing what share
/// of it the tracked sessions account for.
pub fn total_disk_bytes(procs: &[ProcSample]) -> u64 {
    procs.iter().map(|p| p.disk_bytes).sum()
}

/// Names of processes a coding-agent tool call would plausibly spawn (shells,
/// POSIX utilities from Git Bash, interpreters, VCS). Narrows orphan
/// detection to these so it doesn't flag the many ordinary Windows/browser
/// processes that legitimately outlive a launcher parent — extend this list
/// if another tool turns out to leave orphans behind.
const WATCHED_ORPHAN_NAMES: &[&str] = &[
    "find.exe",
    "find",
    "grep.exe",
    "grep",
    "rg.exe",
    "rg",
    "bash.exe",
    "bash",
    "sh.exe",
    "sh",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "node.exe",
    "node",
    "python.exe",
    "python",
    "git.exe",
    "git",
    "curl.exe",
    "wget.exe",
    "npm.cmd",
    "npm",
];

/// A process only really matters once it's used a noticeable amount of CPU
/// or disk since the last poll — a merely-orphaned-but-idle process is
/// harmless and not worth surfacing. Set well above single-poll noise (a
/// stray page fault or filesystem flush) so a process has to be genuinely
/// busy, not just alive, to get flagged — the frontend adds its own
/// multi-poll confirmation on top of this.
const MIN_CPU_PERCENT: f32 = 1.0;
const MIN_DISK_BYTES: u64 = 64 * 1024;

/// Finds processes whose parent is no longer running (Windows never
/// reparents orphans the way Unix inits do — the PPID just points at a PID
/// that's gone) and which are still doing measurable work — the exact shape
/// of the runaway `find /` searches this was built to catch: an agent shells
/// out to a tool, the shell that spawned it exits (or the tool call times
/// out and gets backgrounded), and the tool itself keeps running forever
/// with nothing left to notice it.
pub fn find_runaway_orphans(procs: &[ProcSample]) -> Vec<OrphanProcess> {
    let alive: HashSet<u32> = procs.iter().map(|p| p.pid).collect();

    procs
        .iter()
        .filter(|p| {
            let parent_is_dead = p.parent.is_some_and(|parent| !alive.contains(&parent));
            let is_watched = WATCHED_ORPHAN_NAMES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&p.name));
            let still_active = p.cpu_percent > MIN_CPU_PERCENT || p.disk_bytes > MIN_DISK_BYTES;
            parent_is_dead && is_watched && still_active
        })
        .map(|p| OrphanProcess {
            pid: p.pid,
            name: p.name.clone(),
            cpu_percent: p.cpu_percent,
            disk_bytes: p.disk_bytes,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: Option<u32>, cpu: f32, mem: u64, disk: u64) -> ProcSample {
        named_proc(pid, parent, "unnamed", cpu, mem, disk)
    }

    fn named_proc(
        pid: u32,
        parent: Option<u32>,
        name: &str,
        cpu: f32,
        mem: u64,
        disk: u64,
    ) -> ProcSample {
        ProcSample {
            pid,
            parent,
            name: name.to_string(),
            cpu_percent: cpu,
            memory_bytes: mem,
            disk_bytes: disk,
        }
    }

    #[test]
    fn sums_root_alone_when_no_children() {
        let procs = vec![proc(1, None, 50.0, 1000, 5000)];

        let totals = sum_session_usage(&procs, &[1], 4);

        assert_eq!(totals.cpu_percent, 12.5); // 50 / 4 cores
        assert_eq!(totals.memory_bytes, 1000);
        assert_eq!(totals.disk_bytes, 5000);
    }

    #[test]
    fn includes_descendants_of_root() {
        // 1 -> 2 -> 3 (grandchild), plus an unrelated sibling process 4.
        let procs = vec![
            proc(1, None, 10.0, 100, 1),
            proc(2, Some(1), 20.0, 200, 2),
            proc(3, Some(2), 30.0, 300, 3),
            proc(4, None, 999.0, 999_999, 999_999),
        ];

        let totals = sum_session_usage(&procs, &[1], 1);

        assert_eq!(totals.cpu_percent, 60.0); // 10 + 20 + 30, unrelated pid 4 excluded
        assert_eq!(totals.memory_bytes, 600);
        assert_eq!(totals.disk_bytes, 6);
    }

    #[test]
    fn sums_multiple_independent_roots_without_double_counting_shared_child() {
        // Two tracked sessions (10, 20) both happen to report child 30 as a
        // (stale/duplicate) parent link — should still only count it once.
        let procs = vec![
            proc(10, None, 5.0, 10, 100),
            proc(20, None, 5.0, 10, 100),
            proc(30, Some(10), 5.0, 10, 100),
        ];

        let totals = sum_session_usage(&procs, &[10, 20, 30], 2);

        assert_eq!(totals.cpu_percent, 7.5); // (5 + 5 + 5) / 2 cores, pid 30 counted once
        assert_eq!(totals.memory_bytes, 30);
        assert_eq!(totals.disk_bytes, 300);
    }

    #[test]
    fn missing_root_pid_contributes_nothing() {
        let procs = vec![proc(1, None, 10.0, 100, 100)];

        let totals = sum_session_usage(&procs, &[999], 2);

        assert_eq!(totals, SessionUsageTotals::default());
    }

    #[test]
    fn zero_core_count_is_treated_as_one() {
        let procs = vec![proc(1, None, 10.0, 100, 0)];

        let totals = sum_session_usage(&procs, &[1], 0);

        assert_eq!(totals.cpu_percent, 10.0);
    }

    #[test]
    fn total_disk_bytes_sums_every_process_regardless_of_tracked_roots() {
        let procs = vec![
            proc(1, None, 0.0, 0, 100),
            proc(2, Some(1), 0.0, 0, 50),
            proc(3, None, 0.0, 0, 25), // untracked process, still counted
        ];

        assert_eq!(total_disk_bytes(&procs), 175);
    }

    #[test]
    fn flags_watched_process_with_dead_parent_and_active_cpu() {
        // find.exe (948) claims parent 31972, which isn't in the process list.
        let procs = vec![named_proc(948, Some(31972), "find.exe", 5.0, 0, 0)];

        let orphans = find_runaway_orphans(&procs);

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].pid, 948);
        assert_eq!(orphans[0].name, "find.exe");
    }

    #[test]
    fn flags_watched_process_with_dead_parent_and_active_disk_but_no_cpu() {
        let procs = vec![named_proc(1, Some(999), "find.exe", 0.0, 0, 100_000)];

        let orphans = find_runaway_orphans(&procs);

        assert_eq!(orphans.len(), 1);
    }

    #[test]
    fn does_not_flag_process_with_a_live_parent() {
        let procs = vec![
            named_proc(1, None, "bash.exe", 0.0, 0, 0),
            named_proc(2, Some(1), "find.exe", 50.0, 0, 999_999),
        ];

        assert_eq!(find_runaway_orphans(&procs).len(), 0);
    }

    #[test]
    fn does_not_flag_unwatched_process_names_even_when_orphaned_and_active() {
        // Plenty of ordinary Windows/browser processes legitimately outlive
        // a launcher parent — only watched tool names should ever surface.
        let procs = vec![named_proc(1, Some(999), "chrome.exe", 50.0, 0, 999_999)];

        assert_eq!(find_runaway_orphans(&procs).len(), 0);
    }

    #[test]
    fn does_not_flag_idle_orphan() {
        let procs = vec![named_proc(1, Some(999), "find.exe", 0.0, 0, 0)];

        assert_eq!(find_runaway_orphans(&procs).len(), 0);
    }

    #[test]
    fn name_match_is_case_insensitive() {
        let procs = vec![named_proc(1, Some(999), "FIND.EXE", 5.0, 0, 0)];

        assert_eq!(find_runaway_orphans(&procs).len(), 1);
    }
}
