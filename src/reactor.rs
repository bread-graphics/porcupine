// Boost/Apache2 License

//! The reactor used to process Win32 messages.

use crate::{strict, Error};

use alloc::sync::Arc;
use alloc::task::{Wake};
use core::convert::Infallible;
use core::future::Future;
use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::pin::Pin;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use event_listener::Event as Signal;
use futures_lite::{future, pin};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, GetLastError};
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, ERROR_SUCCESS, WAIT_FAILED};

use windows_sys::Win32::System::Threading::{CreateEventW, GetCurrentProcess, SetEvent};

use windows_sys::Win32::System::WindowsProgramming::INFINITE;

use windows_sys::Win32::UI::WindowsAndMessaging::MSG;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageA, MsgWaitForMultipleObjectsEx, PeekMessageA, TranslateMessage,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PM_REMOVE, QS_ALLINPUT, WM_QUIT};

/// An event indicating that a message has been received.
static MESSAGE_RECEIVED: Signal = Signal::new();

/// Indicate that the current thread has a message.
fn signal_new_message() {
    MESSAGE_RECEIVED.notify_additional(core::usize::MAX);
}

/// Wait until the current thread receives a message.
pub(crate) async fn wait_for_message() {
    MESSAGE_RECEIVED.listen().await;
}

/// The reactor used to process Win32 messages.
pub struct Reactor {
    /// An event that can be signalled to wake up the reactor.
    notify: Arc<Event>,
}

impl Reactor {
    /// Create a new reactor for this variant.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            notify: Arc::new(Event::new()?),
        })
    }

    /// Block on this reactor and run the given future.
    pub fn block_on<R>(self, future: impl Future<Output = R>) -> Result<Option<R>, Error> {
        // Pin ourselves to the stack.
        let this = self;
        pin!(this);

        // Get the waker for this reactor.
        let notify = &this.as_mut().into_ref().notify;
        let waker = Waker::from(notify.clone());

        // Use this context to poll the event.
        let mut context = Context::from_waker(&waker);

        // Begin polling the future.
        pin!(future);
        loop {
            // Poll the future to see if it's ready.
            if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
                return Ok(Some(result));
            }

            // Otherwise, wait for and process window messages.
            loop {
                // Drain all messages from the queue.
                let status = this.as_mut().drain_queue()?;

                // If we need to quit, then we're done.
                if status.quit {
                    return Ok(None);
                }

                // Re-project to get the notify handle.
                let notify = &this.as_mut().into_ref().notify;

                // Wait for either a new message or the notify event.
                let result = unsafe {
                    MsgWaitForMultipleObjectsEx(1, &notify.handle(), INFINITE, QS_ALLINPUT, 0)
                };

                match result {
                    0 => {
                        // The future's waker woke us up. Poll the future again.
                        break;
                    }
                    1 => {
                        // We have new window messages. Drain the queue again.
                        continue;
                    }
                    WAIT_FAILED => {
                        // We failed to wait for the event.
                        return Err(Error::last_error("MsgWaitForMultipleObjectsEx"));
                    }
                    other => {
                        tracing::warn!("Unexpected MsgWaitForMultipleObjectsEx result: {:x}", other)
                    }
                }
            }
        }
    }

    /// Continuously run this reactor until it is shut down.
    pub fn run(self) -> Result<(), Error> {
        self.block_on(future::pending::<Infallible>())
            .map(|t| match t {
                None => {}
                Some(inf) => match inf {},
            })
    }

    /// Drains the message queue for the current thread.
    ///
    /// Returns the number of messages processed.
    ///
    /// Since we can't be moved once we start processing messages, we must be pinned to this thread.
    fn drain_queue(self: Pin<&mut Self>) -> Result<DrainStatus, Error> {
        let mut status = DrainStatus {
            messages: 0,
            quit: false,
        };
        let mut msg_buffer = MaybeUninit::<MSG>::uninit();

        loop {
            // Peek at the next message.
            let has_message = unsafe { PeekMessageA(msg_buffer.as_mut_ptr(), 0, 0, 0, PM_REMOVE) };

            // If there's no message, we're done.
            if has_message <= 0 {
                break;
            }

            // The message is valid, so we can read it.
            let msg = unsafe { &*msg_buffer.as_ptr() };
            status.messages += 1;

            if msg.message == WM_QUIT {
                // If this is a quit message, quit.
                status.quit = true;
                break;
            }

            // Process the message.
            unsafe {
                TranslateMessage(msg);
                DispatchMessageA(msg);
            }

            // Indicate to listeners that we have processed a message.
            signal_new_message();
        }

        Ok(status)
    }
}

/// Handle used for notifying the reactor.
pub(crate) struct Event {
    /// The event handle.
    handle: HANDLE,
}

impl Event {
    /// Creates a new event.
    pub(crate) fn new() -> Result<Self, Error> {
        // Create security attributes that allow this event to be used as a waker.
        // Only needed on Wine, I think?

        // Create the event.
        let handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, ptr::null_mut()) };

        if handle == 0 {
            Err(Error::last_error("CreateEventA"))
        } else {
            Ok(Self { handle })
        }
    }

    /// Wakes up the reactor.
    pub(crate) fn set(&self) -> Result<(), Error> {
        let result = unsafe { SetEvent(self.handle) };

        if result == 0 {
            Err(Error::last_error("SetEvent"))
        } else {
            Ok(())
        }
    }

    /// Try to create a duplicate of this event.
    pub(crate) fn try_clone(&self) -> Result<Self, Error> {
        let mut duplicate = MaybeUninit::uninit();
        let result = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle,
                GetCurrentProcess(),
                duplicate.as_mut_ptr(),
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };

        if result == 0 {
            Err(Error::last_error("DuplicateHandle"))
        } else {
            Ok(Self {
                handle: unsafe { duplicate.assume_init() },
            })
        }
    }

    /// Returns the raw event handle.
    pub(crate) fn raw(&self) -> *const () {
        strict::invalid(self.handle)
    }

    /// Creates a new event from a raw handle.
    ///
    /// # Safety
    ///
    /// The handle must be valid.
    pub(crate) unsafe fn from_raw(handle: *const ()) -> Self {
        Self {
            handle: strict::addr(handle),
        }
    }

    /// Get the handle for this event.
    pub(crate) fn handle(&self) -> HANDLE {
        self.handle
    }

    /// Consumes this event, returning the raw handle.
    pub(crate) fn into_raw(self) -> *const () {
        let handle = self.raw();
        mem::forget(self);
        handle
    }
}

impl Wake for Event {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Err(e) = self.set() {
            tracing::error!("Failed to wake up the reactor: {}", e)
        }
    }
}

unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            if CloseHandle(self.handle) == 0 {
                tracing::warn!(
                    "Failed to close the event handle: {}",
                    Error::last_error("CloseHandle")
                )
            }
        }
    }
}

struct DrainStatus {
    /// The number of messages processed.
    messages: usize,

    /// Whether we need to quit.
    quit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future;

    use std::future::Future;
    use std::pin::Pin;
    use std::task::Context;
    use std::time::Duration;

    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage;

    #[test]
    fn test_event() {
        let event = Event::new().expect("to create event");

        // The event should not be signalled.
        assert_ne!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should not be signalled"
        );

        // Signaling the event should wake it once but not more.
        event.set().expect("to wake event");
        assert_eq!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should be signalled"
        );
        assert_ne!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should not be signalled"
        );

        // Cloning the event should create a new event that can signal the old one.
        let event2 = event.try_clone().expect("to clone event");
        event2.set().expect("to wake event");
        assert_eq!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should be signalled after clone"
        );
        assert_ne!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should not be signalled again"
        );

        // Dropping the duplicate should not close the original.
        drop(event2);
        event.set().expect("to wake event");
        assert_eq!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should be signalled after clone's drop"
        );
        assert_ne!(
            unsafe { WaitForSingleObject(event.handle(), 1) },
            0,
            "event should not be signalled again"
        );

        // This functionality should carry over to the waker.
        let event2 = event.try_clone().expect("to clone event");
        let waker = Waker::from(Arc::new(event2));

        // The async-io thread will wake the event.
        let mut timer = async_io::Timer::after(Duration::from_millis(1000));
        let poll = Pin::new(&mut timer).poll(&mut Context::from_waker(&waker));
        assert!(poll.is_pending());

        // The event should be signalled before the timeout.
        assert_eq!(
            unsafe { WaitForSingleObject(event.handle(), 2000) },
            0,
            "event should be signalled after async-io wake"
        );
    }

    #[test]
    fn test_reactor() {
        let reactor = || Reactor::new().expect("to create a new reactor");

        // Block on a basic ready-future.
        assert_eq!(
            reactor()
                .block_on(future::ready(42))
                .expect("to block on ready"),
            Some(42),
            "ready future should return value"
        );

        // Block on a more involved future.
        assert!(
            reactor()
                .block_on(async_io::Timer::after(Duration::from_millis(1000)))
                .expect("to block on timer")
                .is_some(),
            "timer future should return value"
        );

        // Post a quit message.
        unsafe {
            PostQuitMessage(0);
        }

        // The message loop should quit.
        assert!(
            reactor()
                .block_on(async_io::Timer::after(Duration::from_millis(1000)))
                .expect("to block on timer")
                .is_none(),
            "timer future should return None on quit"
        );
    }
}
