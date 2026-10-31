"""Print the focused element's UI Automation facts (control type, patterns, children)."""
import comtypes.client  # noqa: F401  (pywinauto's dependency)
from pywinauto.uia_defines import IUIA

uia = IUIA().iuia
el = uia.GetFocusedElement()


def describe(e, depth=0):
    import comtypes.gen.UIAutomationClient as U
    pats = []
    for name, pid in (("Text", U.UIA_TextPatternId), ("Value", U.UIA_ValuePatternId), ("Text2", 10024)):
        try:
            if e.GetCurrentPattern(pid):
                pats.append(name)
        except Exception:
            pass
    r = e.CurrentBoundingRectangle
    print("  " * depth + f"{e.CurrentControlType} {e.CurrentClassName!r} name={e.CurrentName[:40]!r} "
          f"patterns={pats} rect=({r.left},{r.top},{r.right},{r.bottom}) focus={e.CurrentHasKeyboardFocus}")


describe(el)
walker = uia.ControlViewWalker
c = walker.GetFirstChildElement(el)
while c:
    describe(c, 1)
    c = walker.GetNextSiblingElement(c)
