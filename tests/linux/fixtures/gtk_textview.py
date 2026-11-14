"""A GTK3 text view for the Linux host gate.

Usage: python3 -I gtk_textview.py <text> <mirror-file>
Shows the text in a TextView with the caret at the start and writes the
buffer to <mirror-file> after every change, so the gate can check an accepted
suggestion from the app's side, independent of what Oumu reads back.
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

text, mirror = sys.argv[1], sys.argv[2]
# On Wayland the app id (Hyprland class) comes from the program name, not set_wmclass.
GLib.set_prgname("oumu-e2e")

win = Gtk.Window(title="oumu-e2e")
win.set_wmclass("oumu-e2e", "oumu-e2e")
win.set_default_size(820, 420)
view = Gtk.TextView()
view.set_wrap_mode(Gtk.WrapMode.WORD)
view.set_left_margin(16)
view.set_top_margin(16)
buf = view.get_buffer()
buf.set_text(text)
buf.place_cursor(buf.get_start_iter())


def write_mirror(b):
    with open(mirror, "w", encoding="utf-8") as f:
        f.write(b.get_text(b.get_start_iter(), b.get_end_iter(), True))


buf.connect("changed", write_mirror)
write_mirror(buf)
win.add(view)
win.connect("destroy", Gtk.main_quit)
win.show_all()
GLib.idle_add(view.grab_focus)
Gtk.main()
