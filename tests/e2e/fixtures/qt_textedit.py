"""A Qt 6 text edit for the Linux host gate (TARGET=qt).

Usage: python3 -I qt_textedit.py <text> <mirror-file>
Same contract as gtk_textview.py: the caret starts at the beginning and the
buffer is written to <mirror-file> after every change.
"""
import os
import sys

from PySide6.QtGui import QFont, QGuiApplication, QTextCursor
from PySide6.QtWidgets import QApplication, QPlainTextEdit, QTextEdit

text, mirror = sys.argv[1], sys.argv[2]
# OUMU_QT_RICH=1: a QTextEdit (window "oumu-e2e-rich") instead of a QPlainTextEdit.
RICH = os.environ.get("OUMU_QT_RICH") == "1"

app = QApplication(sys.argv)
# On Wayland the app id (Hyprland class) comes from the desktop file name.
QGuiApplication.setDesktopFileName("oumu-e2e-qt")
edit = QTextEdit() if RICH else QPlainTextEdit()
if RICH:
    edit.setAcceptRichText(False)
edit.setWindowTitle("oumu-e2e-rich" if RICH else "oumu-e2e")
edit.setFont(QFont("sans-serif", 13))
edit.resize(820, 420)
edit.setPlainText(text)
edit.moveCursor(QTextCursor.MoveOperation.Start)


def write_mirror():
    with open(mirror, "w", encoding="utf-8") as f:
        f.write(edit.toPlainText())


edit.textChanged.connect(write_mirror)
write_mirror()
edit.show()
edit.setFocus()
sys.exit(app.exec())
