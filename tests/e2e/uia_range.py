"""Reproduce ax.rs text_range on the focused element: python uia_range.py START LEN"""
import sys

import comtypes.client  # noqa: F401
from pywinauto.uia_defines import IUIA

import comtypes.gen.UIAutomationClient as U

start, n = int(sys.argv[1]), int(sys.argv[2])
uia = IUIA().iuia
tp = uia.GetFocusedElement().GetCurrentPattern(U.UIA_TextPatternId).QueryInterface(U.IUIAutomationTextPattern)
doc = tp.DocumentRange
text = doc.GetText(-1)
print("want", repr(text[start:start + n]))
r = doc.Clone()
r.MoveEndpointByRange(U.TextPatternRangeEndpoint_End, r, U.TextPatternRangeEndpoint_Start)
moved = r.Move(U.TextUnit_Character, start)
print("Move returned", moved, "asked", start)
got_end = r.MoveEndpointByUnit(U.TextPatternRangeEndpoint_End, U.TextUnit_Character, n)
print("end moved", got_end, "text", repr(r.GetText(-1)))
print("rects", r.GetBoundingRectangles())
# alternative: move the start endpoint instead of the degenerate range
r2 = doc.Clone()
m2 = r2.MoveEndpointByUnit(U.TextPatternRangeEndpoint_Start, U.TextUnit_Character, start)
r2.MoveEndpointByRange(U.TextPatternRangeEndpoint_End, r2, U.TextPatternRangeEndpoint_Start)
r2.MoveEndpointByUnit(U.TextPatternRangeEndpoint_End, U.TextUnit_Character, n)
print("endpoint-move", m2, repr(r2.GetText(-1)), r2.GetBoundingRectangles())
