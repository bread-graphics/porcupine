// Boost/Apache2 License

use crate::module::current_module;
use crate::Error;

use alloc::rc::Rc;

use core::cell::Cell;
use core::marker::{PhantomData, PhantomPinned};
use core::num::NonZeroU32;

use blood_geometry::Point;

use windows_sys::Win32::UI::WindowsAndMessaging::{PostQuitMessage, SetCursorPos};

/// NonZeroU32 as a one.
const ONE: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1) };

/// The client used to send instructions to the system.
#[derive(Clone)]
pub struct Client(Rc<Inner>);

/// Inner data of the client.
struct Inner {
    /// The current number of windows.
    ///
    /// This is set to `None` if no windows have been created yet. Once it reaches Some(0),
    /// the application is set to quit.
    window_count: Cell<Option<NonZeroU32>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Create a new `Client`.
    pub fn new() -> Self {
        Self(Rc::new(Inner {
            window_count: Cell::new(None),
        }))
    }

    /// Get the current number of windows.
    pub fn window_count(&self) -> u32 {
        self.0.window_count.get().map_or(0, |count| count.get())
    }

    /// Set the cursor position.
    pub fn set_cursor_pos(&self, pos: Point<i32>) -> Result<(), Error> {
        let result = unsafe { SetCursorPos(pos.x(), pos.y()) };

        if result == 0 {
            Err(Error::last_error("SetCursorPos"))
        } else {
            Ok(())
        }
    }

    /// Increment the window count.
    pub(crate) fn increment_window_count(&self) {
        let count = self.0.window_count.get().map_or(ONE, |count| unsafe {
            NonZeroU32::new_unchecked({
                count
                    .get()
                    .checked_add(1)
                    .unwrap_or_else(|| abort!("Window count overflowed"))
            })
        });

        self.0.window_count.set(Some(count));
    }

    /// Decrement the window count.
    pub(crate) fn decrement_window_count(&self) {
        let count = match self.0.window_count.get() {
            Some(count) => count,
            None => unreachable!("Count should never be zero in this case"),
        };

        let new_count = count.get().saturating_sub(1);
        match NonZeroU32::new(new_count) {
            None => {
                // No more windows, quit the application.
                unsafe {
                    PostQuitMessage(0);
                }

                self.0.window_count.set(None);
            }
            Some(count) => {
                self.0.window_count.set(Some(count));
            }
        }
    }

    /// Send a quit message to the application.
    pub fn quit(&self) {
        unsafe {
            PostQuitMessage(0);
        }
    }

    /// Wait for an event to occur.
    pub async fn wait_for_event(&self) {
        crate::reactor::wait_for_message().await;
    }
}

#[cfg(feature = "raw-window-handle")]
unsafe impl raw_window_handle::HasRawDisplayHandle for Client {
    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        let handle = raw_window_handle::WindowsDisplayHandle::empty();

        // TODO: Add future fields to handle if needed.

        raw_window_handle::RawDisplayHandle::Windows(handle)
    }
}
