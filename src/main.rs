//! BlackScreen: simula una pantalla apagada.
//! Ventana negra, fullscreen sobre todos los monitores, siempre por delante.
//! Doble Esc (dos pulsaciones en menos de 500 ms) cierra la app.
//!
//! Sin dependencias: Win32 puro via FFI.
//!
//! `no_std` + punto de entrada propio: sin runtime de Rust ni CRT de C, asi el
//! exe pesa unos pocos KB y no necesita VCRUNTIME140.dll. Solo importa
//! user32, gdi32 y kernel32, presentes en cualquier Windows.

#![no_std]
#![no_main]
#![windows_subsystem = "windows"]
#![allow(non_snake_case, clippy::upper_case_acronyms)]

use core::ptr::null;
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::Relaxed};

type HWND = isize;
type HANDLE = isize;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;

#[repr(C)]
struct WNDCLASSW {
    style: u32,
    lpfnWndProc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HANDLE,
    hIcon: HANDLE,
    hCursor: HANDLE,
    hbrBackground: HANDLE,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: WPARAM,
    lParam: LPARAM,
    time: u32,
    pt: [i32; 2],
}

#[repr(C)]
struct KBDLLHOOKSTRUCT {
    vkCode: u32,
    scanCode: u32,
    flags: u32,
    time: u32,
    dwExtraInfo: usize,
}

type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;
type WinEventProc = unsafe extern "system" fn(HANDLE, u32, HWND, i32, i32, u32, u32);

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(wc: *const WNDCLASSW) -> u16;
    fn CreateWindowExW(
        ex: u32, class: *const u16, title: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32,
        parent: HWND, menu: HANDLE, inst: HANDLE, param: *const u8,
    ) -> HWND;
    fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn DestroyWindow(h: HWND) -> i32;
    fn PostQuitMessage(code: i32);
    fn PostMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> i32;
    fn GetMessageW(msg: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn GetSystemMetrics(i: i32) -> i32;
    fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, w: i32, hgt: i32, flags: u32) -> i32;
    fn SetForegroundWindow(h: HWND) -> i32;
    fn SetTimer(h: HWND, id: usize, ms: u32, f: usize) -> usize;
    fn SetCursor(c: HANDLE) -> HANDLE;
    fn SetWindowsHookExW(id: i32, f: HookProc, inst: HANDLE, thread: u32) -> HANDLE;
    fn CallNextHookEx(hook: HANDLE, code: i32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn SetWinEventHook(
        min: u32, max: u32, module: HANDLE, f: WinEventProc,
        pid: u32, tid: u32, flags: u32,
    ) -> HANDLE;
    fn SetProcessDpiAwarenessContext(ctx: isize) -> i32;
    fn GetAsyncKeyState(vk: i32) -> i16;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetStockObject(i: i32) -> HANDLE;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HANDLE;
    fn SetThreadExecutionState(flags: u32) -> u32;
    fn ExitProcess(code: u32) -> !;
}

const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_SETCURSOR: u32 = 0x0020;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_KEYDOWN: usize = 0x0100;
const WM_KEYUP: usize = 0x0101;
const WM_TIMER: u32 = 0x0113;
const VK_ESCAPE: u32 = 0x1B;
const VK_TAB: u32 = 0x09;
const VK_CONTROL: i32 = 0x11;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const LLKHF_ALTDOWN: u32 = 0x20;
// ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
const KEEP_AWAKE: u32 = 0x8000_0000 | 0x2 | 0x1;
const WH_KEYBOARD_LL: i32 = 13;
const HWND_TOPMOST: HWND = -1;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_OBJECT_SHOW: u32 = 0x8002;
const BLACK_BRUSH: i32 = 4;

/// Ventana maxima entre las dos pulsaciones de Esc.
const DOUBLE_ESC_MS: u32 = 500;
/// Red de seguridad: re-afirma el topmost aunque no llegue ningun evento.
const FALLBACK_TIMER_MS: u32 = 250;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static LAST_ESC: AtomicU32 = AtomicU32::new(0);
static ESC_SEEN: AtomicBool = AtomicBool::new(false);
static ESC_HELD: AtomicBool = AtomicBool::new(false);

/// Sube la ventana al tope del z-order sin robar el foco.
unsafe fn raise(hwnd: HWND) {
    SetWindowPos(
        hwnd, HWND_TOPMOST, 0, 0, 0, 0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

/// Cubre el escritorio virtual completo (todos los monitores).
unsafe fn cover_all(hwnd: HWND) {
    SetWindowPos(
        hwnd, HWND_TOPMOST,
        GetSystemMetrics(76), GetSystemMetrics(77), // SM_X/YVIRTUALSCREEN
        GetSystemMetrics(78), GetSystemMetrics(79), // SM_CX/CYVIRTUALSCREEN
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            SetCursor(0); // cursor oculto
            1
        }
        WM_TIMER => {
            raise(hwnd);
            0
        }
        WM_DISPLAYCHANGE => {
            cover_all(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

/// Hook global de teclado: detecta el doble Esc aunque otra ventana tenga el foco.
unsafe extern "system" fn keyboard_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(l as *const KBDLLHOOKSTRUCT);

        // Atajos que abren interfaz del sistema (Inicio, Alt+Tab, Administrador de
        // tareas): se descartan para que nada estatico quede pintado encima.
        let alt = kb.flags & LLKHF_ALTDOWN != 0;
        let ctrl = GetAsyncKeyState(VK_CONTROL) < 0;
        let blocked = match kb.vkCode {
            VK_LWIN | VK_RWIN => true,
            VK_TAB => alt,
            VK_ESCAPE => alt || ctrl,
            _ => false,
        };
        if blocked {
            return 1;
        }

        if kb.vkCode == VK_ESCAPE {
            if w == WM_KEYUP {
                ESC_HELD.store(false, Relaxed);
            } else if w == WM_KEYDOWN && !ESC_HELD.swap(true, Relaxed) {
                // Solo pulsaciones reales: se ignora la auto-repeticion al mantener la tecla.
                let prev = LAST_ESC.swap(kb.time, Relaxed);
                if ESC_SEEN.swap(true, Relaxed) && kb.time.wrapping_sub(prev) <= DOUBLE_ESC_MS {
                    PostMessageW(WINDOW.load(Relaxed), WM_CLOSE, 0, 0);
                }
            }
        } else if w == WM_KEYDOWN {
            ESC_SEEN.store(false, Relaxed); // otra tecla en medio rompe la secuencia
        }
    }
    CallNextHookEx(0, code, w, l)
}

/// Cualquier ventana que se muestre o tome el primer plano nos manda de vuelta arriba.
unsafe extern "system" fn win_event(
    _hook: HANDLE, _event: u32, hwnd: HWND, id_object: i32, _child: i32, _thread: u32, _time: u32,
) {
    let ours = WINDOW.load(Relaxed);
    if id_object == 0 && hwnd != 0 && hwnd != ours {
        raise(ours);
    }
}

/// ASCII -> UTF-16 en tiempo de compilacion (sin Vec ni asignaciones).
const fn wide<const N: usize>(s: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    let mut i = 0;
    while i < N {
        out[i] = s[i] as u16;
        i += 1;
    }
    out
}

static CLASS: [u16; 12] = wide(b"BlackScreen\0");

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { ExitProcess(1) }
}

/// Punto de entrada del exe: el linker lo llama directamente, sin CRT.
/// (`windows_subsystem = "windows"` hace que rustc pida /ENTRY:mainCRTStartup.)
#[no_mangle]
pub extern "system" fn mainCRTStartup() -> ! {
    run();
    unsafe { ExitProcess(0) }
}

fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(-4); // PER_MONITOR_AWARE_V2: coordenadas reales en multi-monitor

        let instance = GetModuleHandleW(null());
        let class = &CLASS;
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: wnd_proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: GetStockObject(BLACK_BRUSH),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        if RegisterClassW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class.as_ptr(), class.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            GetSystemMetrics(76), GetSystemMetrics(77),
            GetSystemMetrics(78), GetSystemMetrics(79),
            0, 0, instance, null(),
        );
        if hwnd == 0 {
            return;
        }
        WINDOW.store(hwnd, Relaxed);
        SetForegroundWindow(hwnd);

        SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_hook, instance, 0);
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0, win_event, 0, 0, 0);
        SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, 0, win_event, 0, 0, 0);
        SetTimer(hwnd, 1, FALLBACK_TIMER_MS, 0);

        // Mientras la app este abierta el equipo no se suspende ni apaga la pantalla
        // (en OLED el negro ya son pixeles apagados). Se revierte solo al salir.
        SetThreadExecutionState(KEEP_AWAKE);

        let mut msg: MSG = core::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        // Los hooks se liberan solos al terminar el proceso.
    }
}
