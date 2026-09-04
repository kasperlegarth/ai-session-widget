pub fn resolve_focus_pid(
    start_pid: u32,
    parent_of: impl Fn(u32) -> Option<u32>,
    has_window: impl Fn(u32) -> bool,
    max_depth: usize,
) -> Option<u32> {
    let mut current = start_pid;
    for _ in 0..=max_depth {
        if has_window(current) {
            return Some(current);
        }
        match parent_of(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
    None
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn returns_start_pid_when_it_owns_a_window() {
        let result = resolve_focus_pid(100, |_| None, |pid| pid == 100, 5);
        assert_eq!(result, Some(100));
    }

    #[test]
    fn walks_up_to_parent_that_owns_a_window() {
        let parents: HashMap<u32, u32> = [(100, 50)].into_iter().collect();
        let windowed = [50];

        let result = resolve_focus_pid(
            100,
            |pid| parents.get(&pid).copied(),
            |pid| windowed.contains(&pid),
            5,
        );

        assert_eq!(result, Some(50));
    }

    #[test]
    fn returns_none_when_no_ancestor_has_a_window() {
        let parents: HashMap<u32, u32> = [(100, 50), (50, 10)].into_iter().collect();

        let result = resolve_focus_pid(100, |pid| parents.get(&pid).copied(), |_| false, 5);

        assert_eq!(result, None);
    }

    #[test]
    fn stops_at_max_depth_to_avoid_infinite_loop() {
        // A cycle (10 -> 20 -> 10 -> ...) would loop forever without a depth cap.
        let parents: HashMap<u32, u32> = [(10, 20), (20, 10)].into_iter().collect();

        let result = resolve_focus_pid(10, |pid| parents.get(&pid).copied(), |_| false, 3);

        assert_eq!(result, None);
    }
}

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
};

struct FindContext {
    target_pid: u32,
    found: Option<HWND>,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut FindContext);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == ctx.target_pid {
        ctx.found = Some(hwnd);
        return BOOL(0); // stop enumeration
    }
    BOOL(1)
}

fn window_for_pid(pid: u32) -> Option<HWND> {
    let mut ctx = FindContext {
        target_pid: pid,
        found: None,
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.found
}

pub fn pid_has_window(pid: u32) -> bool {
    window_for_pid(pid).is_some()
}

/// Finds the visible top-level window owned by `pid` (or, failing that,
/// by the nearest process ancestor that owns one) and brings it to the
/// foreground. No-op if nothing is found.
pub fn focus_pid(pid: u32) {
    use sysinfo::{Pid, System};

    let mut sys = System::new_all();
    sys.refresh_all();

    let parent_of = |p: u32| {
        sys.process(Pid::from_u32(p))
            .and_then(|proc| proc.parent())
            .map(|pp| pp.as_u32())
    };

    let Some(target_pid) = resolve_focus_pid(pid, parent_of, pid_has_window, 10) else {
        return;
    };

    if let Some(hwnd) = window_for_pid(target_pid) {
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}
