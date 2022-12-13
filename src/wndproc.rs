// Boost/Apache2 License

//! This function contains the window procedure for the window.

use crate::abort_on_panic;
use crate::class::ClassData;
use crate::client::Client;
use crate::event::Event;
use crate::strict;
use crate::window::{BorrowedWindow, WindowData};

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::Rc;

use core::cell::RefCell;
use core::mem::ManuallyDrop;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTA;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DefWindowProcA, GetClassLongPtrA, GetWindowLongPtrA, IsWindow, SetWindowLongPtrA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWLP_USERDATA, WM_CREATE, WM_GETMINMAXINFO, WM_NCCREATE, WM_NCDESTROY,
};

use windows_sys::Win32::UI::Shell::DefSubclassProc;

/// The real window procedure, parameterized by the event handler.
pub(crate) unsafe extern "system" fn porcupine_window_procedure<
    'a,
    T: 'a,
    F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>) + 'a,
>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Prevent an unwinding panic from interfering with C code.
    abort_on_panic(move || {
        // Get the client information.
        let ptr = GetClassLongPtrA(hwnd, 0);
        debug_assert_ne!(ptr, 0);

        let data = strict::reconstitute(ptr as isize) as *const ClassData<F>;
        let data = ManuallyDrop::new(Rc::from_raw(data));

        // Handle the message.
        handle_window_message::<T, F>(&data, hwnd, msg, wparam, lparam, false)
    })
}

/// The subclass window procedure for a subclassed window.
pub(crate) unsafe extern "system" fn porcupine_subclass_procedure<
    'a,
    T: 'a,
    F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>) + 'a,
>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_uid: usize,
    ref_data: usize,
) -> LRESULT {
    // Prevent an unwinding panic from interfering with C code.
    abort_on_panic(move || {
        // Get the client information.
        let data = strict::reconstitute(ref_data as isize) as *const ClassData<F>;
        let data = ManuallyDrop::new(Rc::from_raw(data));

        // Handle the message.
        handle_window_message::<T, F>(&*data, hwnd, msg, wparam, lparam, true)
    })
}

fn handle_window_message<'a, T: 'a, F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>) + 'a>(
    client: &Rc<ClassData<F>>,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    is_subclass: bool,
) -> LRESULT {
    // The way to handle messages by default.
    let default_handler = if is_subclass {
        DefSubclassProc
    } else {
        DefWindowProcA
    };

    tracing::trace!(
        "hwnd: {:x}, msg: {:x}, wparam: {:x}, lparam: {:x}",
        hwnd,
        msg,
        wparam,
        lparam
    );

    /// Shorthand for returning with the value from the default event handler.
    macro_rules! bail_default {
        () => {
            return unsafe { default_handler(hwnd, msg, wparam, lparam) };
        };
    }

    // If the handle is null, just skip the message.
    if hwnd == 0 {
        tracing::warn!("Window procedure called with null window handle.");
        bail_default!();
    }

    // Only run this handler for windows.
    if unsafe { IsWindow(hwnd) } == 0 {
        tracing::warn!("Window procedure called with invalid window handle.");
        bail_default!();
    }

    // If the message is WM_NCCREATE, set the user data.
    let window_data = match msg {
        WM_NCCREATE => {
            let create_struct = strict::reconstitute(lparam).cast::<CREATESTRUCTA>();
            debug_assert!(!create_struct.is_null());
            debug_assert!(unsafe { !(*create_struct).lpCreateParams.is_null() });

            // The passed in data will be a Box<T>.
            let user_data = unsafe { Box::from_raw((*create_struct).lpCreateParams as *mut T) };

            // Create the WindowData structure.
            let window_data = Box::new(WindowData::new(hwnd, user_data, client.clone()));

            // Set it as our user data.
            let ptr = strict::expose(Box::into_raw(window_data).cast());

            unsafe { SetWindowLongPtrA(hwnd, GWLP_USERDATA, ptr) };

            bail_default!();
        }
        WM_NCDESTROY => {
            // If the window is being destroyed, remove the user data.
            let user_data = unsafe { SetWindowLongPtrA(hwnd, GWLP_USERDATA, 0) };

            // Drop the boxed data.
            let data = strict::reconstitute(user_data) as *mut WindowData<'a, T>;
            drop(unsafe { Box::from_raw(data) });

            // Decrement the window count. This will send a quit message if the count reaches zero.
            client.client.decrement_window_count();

            bail_default!();
        }
        _ => {
            // Otherwise, get the user data.
            let user_data = unsafe { GetWindowLongPtrA(hwnd, GWLP_USERDATA) };

            if msg == WM_GETMINMAXINFO && user_data == 0 {
                tracing::debug!("WM_GETMINMAXINFO called before WM_NCCREATE");
                bail_default!();
            }

            debug_assert_ne!(user_data, 0);
            let user_ptr = strict::reconstitute(user_data) as *const WindowData<'a, T>;

            unsafe { &*user_ptr }
        }
    };

    // From here on, we can propagate panics.
    window_data.catch_panic(move || {
        // Process all events once we aren't running reentrantly.
        let _instance = window_data.begin();

        // Useful variables for parsing events.
        let ClassData {
            client, handler, ..
        } = &**client;
        let bw = unsafe { BorrowedWindow::from_raw_handle(hwnd) };

        // Parse the event.
        match msg {
            WM_CREATE => {
                window_data.push(Event::Created);
            }
            msg => tracing::debug!("Unhandled message: {:x}", msg),
        }
    });

    // By default, just run the default procedure.
    bail_default!();
}
