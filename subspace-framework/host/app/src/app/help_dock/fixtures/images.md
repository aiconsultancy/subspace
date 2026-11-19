# Image fixture

This host-owned guide exercises image loading before a contents target.

## Rectangles

![First rectangle](help-assets/first.png)

![Second rectangle](help-assets/second.png)

![Third rectangle](help-assets/third.png)

## After the images

The target stays at the top while earlier images load and the header changes.

These paragraphs leave enough content after the target to scroll it to the top
of the narrow help panel. They are test text, not a shipped board guide.

The help panel owns its scroll position. A jump remains attached to its section
until the reader scrolls. The three image sizes give the preceding section
height as their decoded rasters become available.

Keyboard navigation and pointer navigation use the same section targets. The
contents list appears for this guide because it contains three headings.

The text continues below the target so that the viewport can place the heading
at its upper edge rather than clamping the scroll to the document's end.

Changing the header must preserve the selected section. This paragraph also
wraps over several lines at the test window's narrow width.

The final paragraph keeps the document taller than its viewport after a jump.
No fixture needs a board, credentials, a network connection or an external file.
