"""List the clipboard's formats (id, name, size)."""
import ctypes
import ctypes.wintypes as wt

u, k = ctypes.windll.user32, ctypes.windll.kernel32
u.GetClipboardData.restype = wt.HANDLE
k.GlobalSize.argtypes = [wt.HGLOBAL]
k.GlobalSize.restype = ctypes.c_size_t
u.OpenClipboard(None)
f = 0
while True:
    f = u.EnumClipboardFormats(f)
    if not f:
        break
    name = ctypes.create_unicode_buffer(128)
    u.GetClipboardFormatNameW(f, name, 128)
    h = u.GetClipboardData(f)
    print(f, name.value or "(standard)", k.GlobalSize(h) if h else None)
u.CloseClipboard()
