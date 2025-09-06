use crate::common_backend::WindowBackend;
use std::ffi::CString;
use std::ptr;
use std::process::Command;

extern crate x11;
use core::ffi::{c_int, c_uchar, c_ulong};
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

    fn find_window_internal(display: *mut Display, target_title: &str) -> Option<Window> {
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

        let mut result = None;

        if !windows.is_null() {
            for i in 0..window_count {
                let window = unsafe { *windows.add(i as usize) };
                let mut name: *mut i8 = ptr::null_mut();
                unsafe {
                    XFetchName(display, window, &mut name);
                    if !name.is_null() {
                        let c_str = std::ffi::CStr::from_ptr(name);
                        let title = c_str.to_string_lossy().into_owned();
                        XFree(name as *mut _);
                        if title.contains(target_title) {
                            result = Some(window);
                            break;
                        }
                    }
                }
            }
        }

        if !windows.is_null() {
            unsafe { XFree(windows as *mut _) };
        }

        result
    }

    fn is_on_current_workspace_internal(display: *mut Display, window: Window) -> bool {
        let cstring_net_wm_desktop = CString::new("_NET_WM_DESKTOP").unwrap();
        let net_wm_desktop = unsafe { XInternAtom(display, cstring_net_wm_desktop.as_ptr(), 1) };
        let cstring_net_current_desktop = CString::new("_NET_CURRENT_DESKTOP").unwrap();
        let net_current_desktop = unsafe { XInternAtom(display, cstring_net_current_desktop.as_ptr(), 1) };

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
        let mut attributes: XWindowAttributes = unsafe { std::mem::zeroed() };
        unsafe { XGetWindowAttributes(display, window, &mut attributes) };
        attributes.map_state == IsViewable
    }

    fn move_to_current_workspace_internal(display: *mut Display, window: Window) {
        let cstring_net_wm_desktop = CString::new("_NET_WM_DESKTOP").unwrap();
        let net_wm_desktop = unsafe { XInternAtom(display, cstring_net_wm_desktop.as_ptr(), 1) };
        let cstring_net_current_desktop = CString::new("_NET_CURRENT_DESKTOP").unwrap();
        let net_current_desktop = unsafe { XInternAtom(display, cstring_net_current_desktop.as_ptr(), 1) };

        let mut current_desktop: c_ulong = 0;
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

    fn show_internal(display: *mut Display, window: Window) {
        unsafe {
            XMapWindow(display, window);
            XFlush(display);
        }
    }

    fn hide_internal(display: *mut Display, window: Window) {
        unsafe {
            XUnmapWindow(display, window);
            XFlush(display);
        }
    }
}

impl WindowBackend for X11Backend {
    fn find_window(&mut self, app_name: &str) -> Option<u64> {
        Self::with_display(|d| Self::find_window_internal(d, app_name).map(|w| w as u64)).flatten()
    }

    fn is_on_current_workspace(&mut self, window: u64) -> bool {
        Self::with_display(|d| Self::is_on_current_workspace_internal(d, window as Window)).unwrap_or(false)
    }

    fn is_visible(&mut self, window: u64) -> bool {
        Self::with_display(|d| Self::is_visible_internal(d, window as Window)).unwrap_or(false)
    }

    fn move_to_current_workspace(&mut self, window: u64) {
        let _ = Self::with_display(|d| Self::move_to_current_workspace_internal(d, window as Window));
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

