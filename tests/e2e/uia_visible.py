"""Print the focused element's TextPattern visible ranges (as offsets into its text)."""
import comtypes.client  # noqa: F401
from pywinauto.uia_defines import IUIA

import comtypes.gen.UIAutomationClient as U

uia = IUIA().iuia
el = uia.GetFocusedElement()
tp = el.GetCurrentPattern(U.UIA_TextPatternId).QueryInterface(U.IUIAutomationTextPattern)
doc = tp.DocumentRange
text = doc.GetText(-1)
vr = tp.GetVisibleRanges()
print("doc chars", len(text), "visible ranges", vr.Length)
for i in range(vr.Length):
    r = vr.GetElement(i)
    head = doc.Clone()
    head.MoveEndpointByRange(U.TextPatternRangeEndpoint_End, r, U.TextPatternRangeEndpoint_Start)
    start = len(head.GetText(-1))
    t = r.GetText(-1)
    print(f"  [{i}] start {start} len {len(t)}: {t[:50]!r} ... {t[-30:]!r}")
