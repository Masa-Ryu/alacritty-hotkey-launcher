use rdev::{listen, Event, EventType, Key};
use std::process::Command;
use std::time::{Duration, Instant};
extern crate x11;

use core::ffi::c_int;
use core::ffi::c_uchar;
use core::ffi::c_ulong;
use std::ffi::CString;
use std::ptr;
use x11::xlib::*;

struct Config {
    double_press_interval: u64,
    app_path: String,
    app_name: String,
    detect_key: Key,
}

impl Config {
    fn default() -> Self {
        Config {
            double_press_interval: 500,
            app_path: String::from("/usr/local/bin/alacritty"),
            app_name: String::from("Alacritty"),
            detect_key: Key::ControlLeft,
        }
    }
}

fn main() {
    let mut last_press_time: Option<Instant> = None;

    if let Err(error) = listen(move |event| handle_event(event, &mut last_press_time)) {
        println!("Error: {:?}", error);
    }
}

fn handle_event(event: Event, last_press_time: &mut Option<Instant>) {
    let config = Config::default();
    if let EventType::KeyPress(key) = event.event_type {
        if key != config.detect_key {
            return;
        }
        let now = Instant::now();
        if let Some(last_time) = last_press_time {
            if now.duration_since(*last_time) < Duration::from_millis(config.double_press_interval)
            {
                // println!("Double press detected!");
                println!("{}", "-".repeat(10));
                toggle_window();
            }
        }
        *last_press_time = Some(now);
    }
}

fn launch_app() {
    let config = Config::default();
    Command::new(config.app_path)
        .spawn()
        .expect("Failed to start the application");
}

fn find_window_id(display: *mut Display, target_title: &str) -> Option<Window> {
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
                    let title = CString::from_raw(name).into_string().unwrap_or_default();
                    if title.contains(target_title) {
                        result = Some(window);
                        break;
                    }
                }
            }
        }
    }

    unsafe {
        XFree(windows as *mut _);
    }

    result
}

fn is_window_on_current_workspace(display: *mut Display, window: Window) -> bool {
    let net_wm_desktop = unsafe {
        XInternAtom(
            display,
            CString::new("_NET_WM_DESKTOP").unwrap().as_ptr(),
            1,
        )
    };
    let net_current_desktop = unsafe {
        XInternAtom(
            display,
            CString::new("_NET_CURRENT_DESKTOP").unwrap().as_ptr(),
            1,
        )
    };

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

fn move_window_to_current_workspace(display: *mut Display, window: Window) {
    let net_wm_desktop = unsafe {
        XInternAtom(
            display,
            CString::new("_NET_WM_DESKTOP").unwrap().as_ptr(),
            1,
        )
    };
    let net_current_desktop = unsafe {
        XInternAtom(
            display,
            CString::new("_NET_CURRENT_DESKTOP").unwrap().as_ptr(),
            1,
        )
    };

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
    }
}

fn toggle_window_visibility(display: *mut Display, window: Window) {
    let mut attributes: XWindowAttributes = unsafe { std::mem::zeroed() };

    unsafe {
        XGetWindowAttributes(display, window, &mut attributes);
    }

    if attributes.map_state == IsViewable {
        // If window is visible, hidden it
        unsafe {
            XUnmapWindow(display, window);
            XFlush(display);
        }
        println!("Window is hidden.");
    } else {
        // If window is hidden, bring it to front
        unsafe {
            XMapWindow(display, window);
            XFlush(display);
        }
        println!("Window is visible.");
    }
}

fn toggle_window() {
    let display = unsafe { XOpenDisplay(ptr::null()) };
    if display.is_null() {
        eprintln!("X11 cannot open display.");
        return;
    }

    let config = Config::default();
    if let Some(window_id) = find_window_id(display, config.app_name.as_str()) {
        if is_window_on_current_workspace(display, window_id) {
            toggle_window_visibility(display, window_id);
        } else {
            println!("Window is move to workspace.");
            unsafe {
                XUnmapWindow(display, window_id);
                XFlush(display);
             }
            move_window_to_current_workspace(display, window_id);
            unsafe {
                XMapWindow(display, window_id);
                XFlush(display);
            }
            println!("Window is visible after move.");
        }
    } else {
        println!("Window not found.");
        launch_app();
        println!("Window is launched.");
    }

    unsafe { XCloseDisplay(display) };
}
