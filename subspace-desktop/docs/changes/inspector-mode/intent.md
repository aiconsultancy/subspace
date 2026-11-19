# Intent: inspector mode

The owner asked:

> We also want an inspector mode in the main app so we can see the bounds of
> components on hover etc like Chrome devtools and a way to view the
> component's name. We did previously have something similar but it seems to
> have gotten removed. Research on the web and check the code base to see if
> anything remains. Add a menu item for developer settings, maybe even make it
> toggle with F12 or cmd+F12?

The aim is a developer tool that answers "what is this, and where does it come
from?" for any part of the window: the element's bounds, the shared component
it belongs to under the name used in the UI component catalogue and the
Component gallery, and its ID and source location, ready to copy into a
request or a bug report.

The [spec](spec.md) records what remained, the research and the behaviour;
[delivery](delivery.md) records the checks and screenshots; [tasks](tasks.md)
is the checklist.
