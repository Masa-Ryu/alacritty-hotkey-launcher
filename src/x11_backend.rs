use crate::common_backend::WindowBackend;
use crate::x11_ewmh::{
    build_net_active_window_message, build_net_wm_desktop_message, matches_app,
    select_preferred_window, Candidate,
};
use std::ffi::{CStr, CString};
use std::process::Command;
use std::ptr;

extern crate x11;
use core::ffi::{c_int, c_long, c_uchar, c_ulong};
use x11::xlib::*;

pub struct X11Backend;

impl X11Backend {
    pub fn new() -> Self {
        Self
    }

    fn with_display<T, F: FnOnce(*mut Display) -> T>(f: F) -> Option<T> {
        let display = unsafe { XOpenDisplay(ptr::null()) };
        if display.is_null() {
            eprintln!("X11 cannot open display.");
            return None;
        }
        let out = f(display);
        unsafe { XCloseDisplay(display) };
        Some(out)
    }

    fn find_window_internal(display: *mut Display, target: &str) -> Option<Window> {
        unsafe {
            let root = XDefaultRootWindow(display);
            // Try stacking list first for better z-order preference
            let net_client_list_stacking = XInternAtom(
                display,
                CString::new("_NET_CLIENT_LIST_STACKING").unwrap().as_ptr(),
                1,
            );
            let net_client_list = XInternAtom(
                display,
                CString::new("_NET_CLIENT_LIST").unwrap().as_ptr(),
                1,
            );

            let mut list_prop: *mut c_uchar = ptr::null_mut();
            let mut nitems: c_ulong = 0;
            let mut got = false;
            for atom in [net_client_list_stacking, net_client_list] {
                let mut actual_type: Atom = 0;
                let mut actual_format: c_int = 0;
                let mut bytes_after: c_ulong = 0;
                XGetWindowProperty(
                    display,
                    root,
                    atom,
                    0,
                    4096,
                    0,
                    AnyPropertyType as u64,
                    &mut actual_type,
                    &mut actual_format,
                    &mut nitems,
                    &mut bytes_after,
                    &mut list_prop,
                );
                if !list_prop.is_null() && nitems > 0 {
                    got = true;
                    break;
                }
                if !list_prop.is_null() {
                    XFree(list_prop as *mut _);
                    list_prop = ptr::null_mut();
                }
            }

            if !got {
                // Fallback to XQueryTree path
                return Self::find_window_by_query_tree(display, target);
            }

            let windows = list_prop as *const Window;
            let mut candidates: Vec<Candidate> = Vec::new();
            for i in 0..nitems {
                let w = *windows.add(i as usize);
                let title = Self::get_window_title(display, w);
                let class = Self::get_wm_class(display, w);
                if matches_app(target, title.as_deref(), class.as_deref()) {
                    let on_ws = Self::is_on_current_workspace_internal(display, w);
                    let vis = Self::is_visible_internal(display, w);
                    candidates.push(Candidate {
                        window: w,
                        on_current_ws: on_ws,
                        visible: vis,
                    });
                }
            }
            XFree(list_prop as *mut _);

            select_preferred_window(&candidates).map(|id| id as Window)
        }
    }

    fn find_window_by_query_tree(display: *mut Display, target: &str) -> Option<Window> {
        let screen_num = unsafe { XDefaultScreen(display) };
        let mut root = unsafe { XRootWindow(display, screen_num) };

        let mut window_count: u32 = 0;
        let mut windows: *mut Window = ptr::null_mut();
        unsafe {
            XQueryTree(
                display,
                root,
                &mut root,
                &mut root,
                &mut windows,
                &mut window_count,
            );
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        if !windows.is_null() {
            for i in 0..window_count {
                let window = unsafe { *windows.add(i as usize) };
                let title = Self::get_window_title(display, window);
                let class = Self::get_wm_class(display, window);
                if matches_app(target, title.as_deref(), class.as_deref()) {
                    let on_ws = Self::is_on_current_workspace_internal(display, window);
                    let vis = Self::is_visible_internal(display, window);
                    candidates.push(Candidate {
                        window,
                        on_current_ws: on_ws,
                        visible: vis,
                    });
                }
            }
        }
        if !windows.is_null() {
            unsafe { XFree(windows as *mut _) };
        }
        select_preferred_window(&candidates).map(|id| id as Window)
    }

    fn get_window_title(display: *mut Display, window: Window) -> Option<String> {
        unsafe {
            // Try _NET_WM_NAME (UTF8)
            let net_wm_name =
                XInternAtom(display, CString::new("_NET_WM_NAME").unwrap().as_ptr(), 1);
            let utf8 = XInternAtom(display, CString::new("UTF8_STRING").unwrap().as_ptr(), 1);
            let mut actual_type: Atom = 0;
            let mut actual_format: c_int = 0;
            let mut nitems: c_ulong = 0;
            let mut bytes_after: c_ulong = 0;
            let mut prop: *mut c_uchar = ptr::null_mut();
            XGetWindowProperty(
                display,
                window,
                net_wm_name,
                0,
                1024,
                0,
                utf8,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            );
            if !prop.is_null() && nitems > 0 {
                let slice = std::slice::from_raw_parts(prop as *const u8, nitems as usize);
                let title = String::from_utf8_lossy(slice).into_owned();
                XFree(prop as *mut _);
                return Some(title);
            }
            if !prop.is_null() {
                XFree(prop as *mut _);
            }

            // Fallback: WM_NAME via XFetchName
            let mut name: *mut i8 = ptr::null_mut();
            XFetchName(display, window, &mut name);
            if !name.is_null() {
                let c_str = CStr::from_ptr(name);
                let title = c_str.to_string_lossy().into_owned();
                XFree(name as *mut _);
                return Some(title);
            }
        }
        None
    }

    fn get_wm_class(display: *mut Display, window: Window) -> Option<String> {
        unsafe {
            let mut class_hint: XClassHint = std::mem::zeroed();
            if XGetClassHint(display, window, &mut class_hint) != 0 {
                let res_class = if !class_hint.res_class.is_null() {
                    Some(
                        CStr::from_ptr(class_hint.res_class)
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    None
                };
                if !class_hint.res_name.is_null() {
                    XFree(class_hint.res_name as *mut _);
                }
                if !class_hint.res_class.is_null() {
                    XFree(class_hint.res_class as *mut _);
                }
                return res_class;
            }
        }
        None
    }

    fn is_on_current_workspace_internal(display: *mut Display, window: Window) -> bool {
        let cstring_net_wm_desktop = CString::new("_NET_WM_DESKTOP").unwrap();
        let net_wm_desktop = unsafe { XInternAtom(display, cstring_net_wm_desktop.as_ptr(), 1) };
        let cstring_net_current_desktop = CString::new("_NET_CURRENT_DESKTOP").unwrap();
        let net_current_desktop =
            unsafe { XInternAtom(display, cstring_net_current_desktop.as_ptr(), 1) };

        let mut current_desktop: c_ulong = 0;
        let mut window_desktop: c_ulong = 0;

        let root = unsafe { XDefaultRootWindow(display) };

        let mut actual_type_return: Atom = 0;
        let mut actual_format_return: c_int = 0;
        let mut nitems_return: c_ulong = 0;
        let mut bytes_after_return: c_ulong = 0;
        let mut prop_return: *mut c_uchar = ptr::null_mut();

        unsafe {
            XGetWindowProperty(
                display,
                root,
                net_current_desktop,
                0,
                1,
                0,
                AnyPropertyType as u64,
                &mut actual_type_return,
                &mut actual_format_return,
                &mut nitems_return,
                &mut bytes_after_return,
                &mut prop_return,
            );
        }

        if !prop_return.is_null() {
            current_desktop = unsafe { *(prop_return as *mut c_ulong) };
            unsafe { XFree(prop_return as *mut _) };
        }

        unsafe {
            XGetWindowProperty(
                display,
                window,
                net_wm_desktop,
                0,
                1,
                0,
                AnyPropertyType as u64,
                &mut actual_type_return,
                &mut actual_format_return,
                &mut nitems_return,
                &mut bytes_after_return,
                &mut prop_return,
            );
        }

        if !prop_return.is_null() {
            window_desktop = unsafe { *(prop_return as *mut c_ulong) };
            unsafe { XFree(prop_return as *mut _) };
        }

        current_desktop == window_desktop
    }

    fn is_visible_internal(display: *mut Display, window: Window) -> bool {
        // Prefer ICCCM WM_STATE's IconicState to detect minimized windows
        unsafe {
            let wm_state_atom = XInternAtom(display, CString::new("WM_STATE").unwrap().as_ptr(), 1);
            let mut actual_type: Atom = 0;
            let mut actual_format: c_int = 0;
            let mut nitems: c_ulong = 0;
            let mut bytes_after: c_ulong = 0;
            let mut prop: *mut c_uchar = ptr::null_mut();
            XGetWindowProperty(
                display,
                window,
                wm_state_atom,
                0,
                2,
                0,
                AnyPropertyType as u64,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            );
            if !prop.is_null() && nitems >= 1 {
                let state = *(prop as *const c_ulong) as c_long;
                XFree(prop as *mut _);
                if state == 3 {
                    // IconicState
                    return false;
                }
            } else if !prop.is_null() {
                XFree(prop as *mut _);
            }
        }
        let mut attributes: XWindowAttributes = unsafe { std::mem::zeroed() };
        unsafe { XGetWindowAttributes(display, window, &mut attributes) };
        attributes.map_state == IsViewable
    }

    fn move_to_current_workspace_internal(display: *mut Display, window: Window) {
        // Read current desktop
        let net_current_desktop = unsafe {
            XInternAtom(
                display,
                CString::new("_NET_CURRENT_DESKTOP").unwrap().as_ptr(),
                1,
            )
        };
        let root = unsafe { XDefaultRootWindow(display) };
        let mut actual_type: Atom = 0;
        let mut actual_format: c_int = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut c_uchar = ptr::null_mut();
        unsafe {
            XGetWindowProperty(
                display,
                root,
                net_current_desktop,
                0,
                1,
                0,
                AnyPropertyType as u64,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            );
        }
        let mut current_desktop: c_ulong = 0;
        if !prop.is_null() {
            unsafe {
                current_desktop = *(prop as *mut c_ulong);
                XFree(prop as *mut _);
            }
        }
        // EWMH: send ClientMessage _NET_WM_DESKTOP to move, fallback to direct property
        unsafe {
            if Self::ewmh_supported(display) {
                let net_wm_desktop = XInternAtom(
                    display,
                    CString::new("_NET_WM_DESKTOP").unwrap().as_ptr(),
                    1,
                );
                let spec = build_net_wm_desktop_message(
                    window,
                    current_desktop as u64,
                    net_wm_desktop as u64,
                );
                Self::send_client_message(display, root, window, spec);
            } else {
                let net_wm_desktop = XInternAtom(
                    display,
                    CString::new("_NET_WM_DESKTOP").unwrap().as_ptr(),
                    1,
                );
                XChangeProperty(
                    display,
                    window,
                    net_wm_desktop,
                    XA_CARDINAL,
                    32,
                    PropModeReplace,
                    &current_desktop as *const c_ulong as *const u8,
                    1,
                );
                XFlush(display);
            }
        }
    }

    fn show_internal(display: *mut Display, window: Window) {
        unsafe {
            let root = XDefaultRootWindow(display);
            if Self::ewmh_supported(display) {
                let net_active = XInternAtom(
                    display,
                    CString::new("_NET_ACTIVE_WINDOW").unwrap().as_ptr(),
                    1,
                );
                let spec = build_net_active_window_message(window, net_active as u64);
                Self::send_client_message(display, root, window, spec);
            } else {
                XMapWindow(display, window);
                XFlush(display);
            }
        }
    }

    fn hide_internal(display: *mut Display, window: Window) {
        unsafe {
            if Self::ewmh_supported(display) {
                let screen = XDefaultScreen(display);
                XIconifyWindow(display, window, screen);
                XFlush(display);
            } else {
                XUnmapWindow(display, window);
                XFlush(display);
            }
        }
    }

    unsafe fn send_client_message(
        display: *mut Display,
        root: Window,
        window: Window,
        spec: crate::x11_ewmh::ClientMessageSpec,
    ) {
        let mut xev: XClientMessageEvent = std::mem::zeroed();
        xev.type_ = ClientMessage;
        xev.window = window;
        xev.message_type = spec.message_type_atom;
        xev.format = 32;
        let longs: [c_long; 5] = [
            spec.data[0] as c_long,
            spec.data[1] as c_long,
            spec.data[2] as c_long,
            spec.data[3] as c_long,
            spec.data[4] as c_long,
        ];
        xev.data = ClientMessageData::from(longs);
        XSendEvent(
            display,
            root as c_ulong,
            0,
            (SubstructureRedirectMask | SubstructureNotifyMask) as c_long,
            &mut xev as *mut XClientMessageEvent as *mut XEvent,
        );
        XFlush(display);
    }

    unsafe fn ewmh_supported(display: *mut Display) -> bool {
        let root = XDefaultRootWindow(display);
        let net_supported =
            XInternAtom(display, CString::new("_NET_SUPPORTED").unwrap().as_ptr(), 1);
        let mut actual_type: Atom = 0;
        let mut actual_format: c_int = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut c_uchar = std::ptr::null_mut();
        XGetWindowProperty(
            display,
            root,
            net_supported,
            0,
            4096,
            0,
            AnyPropertyType as u64,
            &mut actual_type,
            &mut actual_format,
            &mut nitems,
            &mut bytes_after,
            &mut prop,
        );
        if prop.is_null() || nitems == 0 {
            return false;
        }
        let slice = std::slice::from_raw_parts(prop as *const c_ulong, nitems as usize);
        let required = [
            XInternAtom(
                display,
                CString::new("_NET_WM_DESKTOP").unwrap().as_ptr(),
                1,
            ) as u64,
            XInternAtom(
                display,
                CString::new("_NET_ACTIVE_WINDOW").unwrap().as_ptr(),
                1,
            ) as u64,
        ];
        let supported: Vec<u64> = slice.to_vec();
        let ok = crate::x11_ewmh::have_atoms(&supported, &required);
        XFree(prop as *mut _);
        ok
    }
}

impl WindowBackend for X11Backend {
    fn find_window(&mut self, app_name: &str) -> Option<u64> {
        Self::with_display(|d| Self::find_window_internal(d, app_name)).flatten()
    }

    fn is_on_current_workspace(&mut self, window: u64) -> bool {
        Self::with_display(|d| Self::is_on_current_workspace_internal(d, window as Window))
            .unwrap_or(false)
    }

    fn is_visible(&mut self, window: u64) -> bool {
        Self::with_display(|d| Self::is_visible_internal(d, window as Window)).unwrap_or(false)
    }

    fn move_to_current_workspace(&mut self, window: u64) {
        let _ =
            Self::with_display(|d| Self::move_to_current_workspace_internal(d, window as Window));
    }

    fn show(&mut self, window: u64) {
        let _ = Self::with_display(|d| Self::show_internal(d, window as Window));
    }

    fn hide(&mut self, window: u64) {
        let _ = Self::with_display(|d| Self::hide_internal(d, window as Window));
    }

    fn launch_app(&mut self, app_path: &str) {
        let _ = Command::new(app_path).spawn();
    }
}
