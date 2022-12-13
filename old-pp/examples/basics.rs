/* -----------------------------------------------------------------------------------
 * examples/basics.rs - A basic test of Win32 functionality.
 * porcupine - Safe wrapper around the graphical parts of Win32.
 * Copyright © 2020 not_a_seagull
 *
 * This project is licensed under either the Apache 2.0 license or the MIT license, at
 * your option. For more information, please consult the LICENSE-APACHE or LICENSE-MIT
 * files in the repository root.
 * -----------------------------------------------------------------------------------
 * MIT License:
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the “Software”), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 * -----------------------------------------------------------------------------------
 * Apache 2.0 License Declaration:
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 * ----------------------------------------------------------------------------------
 */

use euclid::rect;
use porcupine::{
    prelude::*, winuser, CmdShow, DroplessWindow, ExtendedWindowStyle, OwnedWindowClass, Window, WindowStyle,
    HWND, LPARAM, LRESULT, UINT, WPARAM,
};

unsafe extern "system" fn window_procedure(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let _w = DroplessWindow::new(hwnd);

    match msg {
        winuser::WM_CLOSE => { winuser::DestroyWindow(hwnd); },
        winuser::WM_DESTROY => winuser::PostQuitMessage(0),
        _ => return winuser::DefWindowProcA(hwnd, msg, wparam, lparam),
    }

    0
}

fn main() -> porcupine::Result<()> {
    // register the window class
    let wc_name = "PorcupineBasicsTest".to_string();
    let mut wc = OwnedWindowClass::new(wc_name);
    wc.set_window_proc(Some(window_procedure));
    wc.register()?;

    // create the window
    let w = Window::new(
        &wc,
        "Hello world!",
        WindowStyle::OVERLAPPED_WINDOW,
        ExtendedWindowStyle::NONE,
        rect(0, 0, 400, 200),
        None,
    )?;
    w.show(CmdShow::Show);
    w.update()?;

    // create the event loop
    while let Some(ref msg) = porcupine::get_message()? {
        porcupine::translate_message(msg);
        porcupine::dispatch_message(msg);
    }

    Ok(())
}
