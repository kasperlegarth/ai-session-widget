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

use std::mem::size_of;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, SetWindowRgn, RGN_DIFF,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EnumWindows, GetForegroundWindow, GetMessageW, GetWindowRect, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, LWA_ALPHA, MSG, SW_RESTORE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_CLOSE, WM_DESTROY, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

struct FindAllContext {
    target_pid: u32,
    found: Vec<HWND>,
}

unsafe extern "system" fn enum_proc_collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut FindAllContext);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == ctx.target_pid {
        ctx.found.push(hwnd);
    }
    BOOL(1) // keep going, collect every match
}

/// Every visible top-level window owned by `pid` — there can be more than
/// one (e.g. several separate Windows Terminal windows all hosted by the
/// same WindowsTerminal.exe process).
fn all_windows_for_pid(pid: u32) -> Vec<HWND> {
    let mut ctx = FindAllContext {
        target_pid: pid,
        found: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(enum_proc_collect), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.found
}

fn window_for_pid(pid: u32) -> Option<HWND> {
    all_windows_for_pid(pid).into_iter().next()
}

pub fn pid_has_window(pid: u32) -> bool {
    window_for_pid(pid).is_some()
}

/// Best-effort check of whether any UI Automation element under `hwnd`
/// (e.g. a Windows Terminal tab title, or visible pane text) contains
/// `hint` (case-insensitive). Used only to disambiguate between several
/// windows that share one process id — Win32 has no direct "which window
/// hosts this child process" API for apps like Windows Terminal that
/// multiplex several windows/panes through one process via ConPTY, so this
/// reads the window's own accessibility tree instead. Returns false (never
/// panics) if UI Automation is unavailable or the walk fails for any reason.
fn window_text_matches_hint(automation: &IUIAutomation, hwnd: HWND, hint_lower: &str) -> bool {
    unsafe {
        let Ok(element) = automation.ElementFromHandle(hwnd) else {
            return false;
        };
        let Ok(condition) = automation.CreateTrueCondition() else {
            return false;
        };
        let Ok(all) = element.FindAll(TreeScope_Descendants, &condition) else {
            return false;
        };
        let Ok(count) = all.Length() else {
            return false;
        };
        for i in 0..count {
            let Ok(item) = all.GetElement(i) else {
                continue;
            };
            let Ok(name) = item.CurrentName() else {
                continue;
            };
            if name.to_string().to_lowercase().contains(hint_lower) {
                return true;
            }
        }
    }
    false
}

/// Picks the right window among several candidates sharing one process id
/// by searching each one's UI Automation tree for `hint` (e.g. the
/// session's project folder name). Falls back to the first candidate if
/// there's only one, if UI Automation can't be initialized, or if none of
/// them match — a wrong-but-present window beats a silent no-op.
fn find_best_window(pid: u32, hint: &str) -> Option<HWND> {
    let candidates = all_windows_for_pid(pid);
    if candidates.len() <= 1 || hint.trim().is_empty() {
        return candidates.into_iter().next();
    }

    let hint_lower = hint.to_lowercase();
    let automation: Option<IUIAutomation> = unsafe {
        // COINIT_APARTMENTTHREADED can legitimately return an "already
        // initialized" error on a thread that's used this before — that's
        // fine, CoCreateInstance below still works either way.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
    };

    if let Some(automation) = &automation {
        for hwnd in &candidates {
            if window_text_matches_hint(automation, *hwnd, &hint_lower) {
                return Some(*hwnd);
            }
        }
    }

    candidates.into_iter().next()
}

/// Finds the visible top-level window owned by `pid` (or, failing that, by
/// the nearest process ancestor that owns one) and brings it to the
/// foreground. If that owner has several windows open (common for apps
/// like Windows Terminal that host multiple windows from one process),
/// `hint` (typically the session's project folder name) is used to pick
/// the right one via UI Automation — see `find_best_window`. No-op if
/// nothing is found at all.
pub fn focus_pid(pid: u32, hint: &str) {
    use sysinfo::{Pid, System};

    // `new_all()` already performs a full refresh at construction — calling
    // `refresh_all()` again right after scanned the entire process table
    // twice for no reason.
    let sys = System::new_all();

    let parent_of = |p: u32| {
        sys.process(Pid::from_u32(p))
            .and_then(|proc| proc.parent())
            .map(|pp| pp.as_u32())
    };

    let Some(target_pid) = resolve_focus_pid(pid, parent_of, pid_has_window, 10) else {
        return;
    };

    if let Some(hwnd) = find_best_window(target_pid, hint) {
        let previous_foreground = unsafe { GetForegroundWindow() };
        unsafe {
            force_foreground(hwnd);
        }
        // `hwnd` itself can be a hidden, zero-size proxy window — notably
        // ConPTY's "PseudoConsoleWindow", which Windows Terminal keeps one
        // of per pane purely so legacy console APIs have something to
        // resolve, and which is what makes SetForegroundWindow land on the
        // right actual terminal window/tab in the first place (each pane
        // has its own such window, one-to-one, so there's nothing to
        // disambiguate — no UI Automation search needed). Highlighting that
        // hwnd draws a 0x0 overlay nobody can see, so re-read whatever the
        // OS actually put in the foreground and highlight that instead.
        //
        // That handoff from the hidden proxy to the real, visible window
        // isn't synchronous with SetForegroundWindow returning — Windows
        // Terminal raises the real window a beat later. Reading
        // GetForegroundWindow immediately afterward can still catch the
        // previous window, or the same invisible proxy, which is exactly
        // what made the highlight silently disappear even though focus
        // itself landed correctly. Poll briefly for a foreground window
        // actually owned by `target_pid` before falling back to whatever's
        // current.
        let visible_hwnd = wait_for_new_foreground(previous_foreground, target_pid);
        highlight_window(visible_hwnd);
    }
}

const FOREGROUND_POLL_INTERVAL: Duration = Duration::from_millis(15);
const FOREGROUND_WAIT_TIMEOUT: Duration = Duration::from_millis(400);

fn wait_for_new_foreground(previous: HWND, target_pid: u32) -> HWND {
    // If the target was already the foreground window before we asked
    // (e.g. the user clicked a card for the terminal they're already
    // looking at), nothing is going to change — polling for a transition
    // that isn't coming just burns the full timeout for no reason.
    let mut previous_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(previous, Some(&mut previous_pid));
    }
    if previous_pid == target_pid {
        return previous;
    }

    let deadline = Instant::now() + FOREGROUND_WAIT_TIMEOUT;
    loop {
        let current = unsafe { GetForegroundWindow() };
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(current, Some(&mut pid));
        }
        if current != previous && pid == target_pid {
            return current;
        }
        if Instant::now() >= deadline {
            return current;
        }
        thread::sleep(FOREGROUND_POLL_INTERVAL);
    }
}

/// How long the highlight overlay stays on screen — long enough to spot at
/// a glance among several open terminal windows, short enough to not linger
/// and look stuck.
const HIGHLIGHT_DURATION_MS: u32 = 3000;

/// Frame thickness of the highlight overlay, in pixels.
const HIGHLIGHT_THICKNESS: i32 = 6;

// COLORREF is 0x00BBGGRR, not RGB — this is a vivid orange (RGB 255,140,0),
// chosen to stand out against both light and dark terminal themes.
const HIGHLIGHT_COLOR: COLORREF = COLORREF(0x00_00_8C_FF);

const OVERLAY_CLASS_NAME: windows::core::PCWSTR = w!("ClawdFocusHighlightOverlay");

unsafe extern "system" fn overlay_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // WM_TIMER is the overlay's own natural expiry; WM_CLOSE is how a
        // newer overlay (see ACTIVE_OVERLAY_HWND) asks an older, still-alive
        // one to get out of the way immediately instead of leaving two
        // overlays alive at once.
        WM_TIMER | WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// HWND (as its raw pointer value) of whichever highlight overlay is
/// currently showing, or 0 if none. Only one overlay is ever meant to be on
/// screen at a time — see `highlight_window`.
static ACTIVE_OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

fn ensure_overlay_class_registered() {
    static REGISTERED: Once = Once::new();
    REGISTERED.call_once(|| unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wndproc),
            hInstance: module.into(),
            hbrBackground: CreateSolidBrush(HIGHLIGHT_COLOR),
            lpszClassName: OVERLAY_CLASS_NAME,
            ..Default::default()
        };
        RegisterClassExW(&class);
    });
}

/// `GetWindowRect` reports a window's *legacy* bounds, which for a
/// DWM-composed window include an invisible resize-border margin — several
/// pixels wide, and pushed even further off-screen for a maximized window —
/// that isn't part of what's actually drawn on screen. Positioning an
/// overlay from that rect lands its frame partly or entirely in that
/// invisible margin. `DWMWA_EXTENDED_FRAME_BOUNDS` reports the real visible
/// bounds instead; fall back to `GetWindowRect` if DWM can't answer (no
/// worse than before), and treat a degenerate (zero-area) result from
/// either as no answer at all.
fn visual_window_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut dwm_rect = RECT::default();
        let dwm_ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut dwm_rect as *mut _ as *mut _,
            size_of::<RECT>() as u32,
        )
        .is_ok()
            && dwm_rect.right > dwm_rect.left
            && dwm_rect.bottom > dwm_rect.top;
        if dwm_ok {
            return Some(dwm_rect);
        }

        let mut gwr_rect = RECT::default();
        let gwr_ok = GetWindowRect(hwnd, &mut gwr_rect).is_ok()
            && gwr_rect.right > gwr_rect.left
            && gwr_rect.bottom > gwr_rect.top;
        gwr_ok.then_some(gwr_rect)
    }
}

/// Briefly draws a thick colored frame directly over `hwnd`'s on-screen
/// bounds — a hollow rectangle (via `SetWindowRgn`) so the window's own
/// content stays visible underneath — then destroys itself after
/// `HIGHLIGHT_DURATION_MS`.
///
/// This exists instead of the "obvious" native approach
/// (`DWMWA_BORDER_COLOR`, which lets DWM color a window's real border): that
/// call succeeds but is invisible in practice — apps like Windows Terminal
/// can overwrite it with their own focus-color logic, and maximized windows
/// hide their real border a few pixels off-screen. An owned topmost overlay
/// sidesteps both: it draws on top of whatever's there and follows the
/// window's actual screen rect regardless of maximize state.
///
/// Runs on its own thread with its own message loop, since the overlay
/// needs to pump `WM_TIMER`/`WM_DESTROY` independently of the caller.
fn highlight_window(hwnd: HWND) {
    // A window that just got raised from minimized/background can briefly
    // report a degenerate rect while DWM finishes the restore — retry a
    // few times rather than silently giving up on the first miss.
    let mut rect = None;
    for attempt in 0..5 {
        rect = visual_window_rect(hwnd);
        if rect.is_some() || attempt == 4 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let Some(rect) = rect else {
        return;
    };

    thread::spawn(move || unsafe {
        ensure_overlay_class_registered();

        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };

        let Ok(overlay) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            OVERLAY_CLASS_NAME,
            w!(""),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            module,
            None,
        ) else {
            return;
        };

        // If a previous highlight is still on screen (e.g. a session was
        // clicked again before the last one's 3s timer ran out), ask it to
        // close right away rather than leaving two overlays alive at once —
        // DestroyWindow can only be called by the thread that created the
        // window, so this asks its own thread to do it via WM_CLOSE.
        let previous = ACTIVE_OVERLAY_HWND.swap(overlay.0 as isize, Ordering::SeqCst);
        if previous != 0 {
            let _ = PostMessageW(HWND(previous as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }

        // Punch out the interior so only a `HIGHLIGHT_THICKNESS`-wide frame
        // around the edge is actually drawn — the window underneath stays
        // fully visible and click-through (WS_EX_TRANSPARENT) inside it.
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let outer = CreateRectRgn(0, 0, width, height);
        let inner = CreateRectRgn(
            HIGHLIGHT_THICKNESS,
            HIGHLIGHT_THICKNESS,
            width - HIGHLIGHT_THICKNESS,
            height - HIGHLIGHT_THICKNESS,
        );
        CombineRgn(outer, outer, inner, RGN_DIFF);
        // SetWindowRgn takes ownership of `outer` (must not be deleted after
        // a successful call) — but `inner` was only a scratch input to
        // CombineRgn and is never handed to Windows, so it has to be deleted
        // here or it leaks a GDI region handle on every single highlight.
        let _ = DeleteObject(inner);
        SetWindowRgn(overlay, outer, BOOL(1));

        let _ = SetLayeredWindowAttributes(overlay, COLORREF(0), 255, LWA_ALPHA);
        let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
        // Belt-and-suspenders: WS_EX_TOPMOST at creation time doesn't always
        // win against other topmost surfaces already on screen (our own
        // always-on-top widget included) — explicitly reassert top-of-band.
        let _ = SetWindowPos(
            overlay,
            HWND_TOPMOST,
            rect.left,
            rect.top,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        SetTimer(overlay, 1, HIGHLIGHT_DURATION_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Clear our slot, but only if a newer overlay hasn't already
        // claimed it (it would have, via the swap above, if we were closed
        // by WM_CLOSE rather than our own timer).
        let _ = ACTIVE_OVERLAY_HWND.compare_exchange(
            overlay.0 as isize,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    });
}

/// Windows normally refuses SetForegroundWindow from a background process
/// (the "foreground lock") unless the calling thread's input state is
/// attached to the currently-focused thread's — this is the standard
/// workaround: briefly attach, steal focus, detach. Without it,
/// SetForegroundWindow fails silently (no error, window just never comes
/// forward), which is exactly what made this a no-op for windows owned by
/// a background process like a Windows Terminal instance the user hadn't
/// just clicked into.
unsafe fn force_foreground(hwnd: HWND) {
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }

    let foreground = GetForegroundWindow();
    let current_thread = GetCurrentThreadId();
    let foreground_thread = GetWindowThreadProcessId(foreground, None);

    let attached = foreground_thread != 0
        && foreground_thread != current_thread
        && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);

    if attached {
        let _ = AttachThreadInput(current_thread, foreground_thread, false);
    }
}
