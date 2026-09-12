"""Controlled native Windows fixture; stdin/stdout JSON protocol, no dependencies."""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import queue
import sys
import threading
import time

user = c.WinDLL("user32", use_last_error=True)
user.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD,
                               c.c_int, c.c_int, c.c_int, c.c_int,
                               w.HWND, w.HMENU, w.HINSTANCE, c.c_void_p]
user.CreateWindowExW.restype = w.HWND
user.SetWindowTextW.argtypes = [w.HWND, w.LPCWSTR]
user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
user.DestroyWindow.argtypes = [w.HWND]
user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
user.PeekMessageW.argtypes = [c.POINTER(w.MSG), w.HWND, w.UINT, w.UINT, w.UINT]
user.TranslateMessage.argtypes = [c.POINTER(w.MSG)]
user.DispatchMessageW.argtypes = [c.POINTER(w.MSG)]
user.DispatchMessageW.restype = c.c_ssize_t

session = Path(os.environ["BROMIUM_FIXTURE_SESSION"]) if os.environ.get("BROMIUM_FIXTURE_SESSION") else None
identity = session.name if session else str(os.getpid())
titles = [f"Bromium Incremental A {identity}", f"Bromium Incremental B {identity}"]
windows = [user.CreateWindowExW(0, "STATIC", title, 0x10CF0000,
                              100 + i * 160, 100 + i * 80, 420, 280,
                              None, None, None, None) for i, title in enumerate(titles)]
button = user.CreateWindowExW(0, "BUTTON", "Before", 0x50000000,
                             30, 50, 120, 32, windows[0], None, None, None)
edit = user.CreateWindowExW(0, "EDIT", "", 0x50800000,
                           30, 100, 240, 32, windows[0], None, None, None)
if not all(windows) or not button or not edit:
    raise c.WinError(c.get_last_error())

commands = queue.Queue()
def read_commands():
    for line in sys.stdin:
        commands.put(json.loads(line))
    commands.put({"command": "quit"})
if not session:
    threading.Thread(target=read_commands, daemon=True).start()
ready = json.dumps({"titles": titles, "button": int(button), "pid": os.getpid()})
if session:
    (session / "ready.json").write_text(ready, encoding="utf-8")
else:
    print(ready, flush=True)
running = True
last_text = ""
expires = time.monotonic() + 60
try:
    while running:
        if session and ((session / "stop").exists() or time.monotonic() >= expires):
            break
        msg = w.MSG()
        while user.PeekMessageW(c.byref(msg), None, 0, 0, 1):
            user.TranslateMessage(c.byref(msg))
            user.DispatchMessageW(c.byref(msg))
        text = c.create_unicode_buffer(512)
        user.GetWindowTextW(edit, text, len(text))
        if text.value != last_text:
            last_text = text.value
            if button:
                user.SetWindowTextW(button, "Result: " + last_text)
        try:
            command = commands.get_nowait()["command"]
        except queue.Empty:
            time.sleep(0.01)
            continue
        if command == "rename":
            user.SetWindowTextW(button, "After")
        elif command == "remove":
            user.DestroyWindow(button)
            button = None
        elif command == "replace":
            if button:
                user.DestroyWindow(button)
            button = user.CreateWindowExW(0, "BUTTON", "Replacement", 0x50000000,
                                         30, 50, 120, 32, windows[0], None, None, None)
            if not button:
                raise c.WinError(c.get_last_error())
        elif command == "move":
            user.SetWindowPos(windows[0], None, 300, 300, 420, 280, 0x0004)
        elif command == "pause":
            pass
        elif command == "quit":
            running = False
        else:
            raise ValueError(command)
        if running:
            print(json.dumps({"done": command}), flush=True)
        if command == "pause":
            time.sleep(0.4)  # Controlled provider delay for GIL tests.
finally:
    for window in windows:
        user.DestroyWindow(window)
    if session:
        (session / "done").touch()
