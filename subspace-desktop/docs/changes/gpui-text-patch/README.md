# Selectable find results and link menus

The desktop enables the [Framework's GPUI Kit text patch](../../../../subspace-framework/docs/changes/gpui-text-patch/README.md).
A secondary click on native Markdown uses the link destination reported by
GPUI Kit's `TextView::on_link_click` callback to offer **Copy link**.

## Evidence

Both captures use the same Component gallery fixture in Cupertino Dark on
macOS. The before build uses the integration baseline from the start of
this change with the same fixture text as the after build.

| Interaction | Before | After |
| --- | --- | --- |
| Drag across the start of a plain value with find active | [Only the match is highlighted](evidence/before-find.png) | [“Select this” is selected and “needle” stays highlighted](evidence/after-find.png) |
| Secondary-click a selected Markdown link | [The text menu offers selection actions](evidence/before-link-menu.png) | [The menu also offers Copy link](evidence/after-link-menu.png) |

```sh
CAPTURE_STATE='{"app.dashboard":"component-gallery","component-gallery.section":"content"}' \
  make capture OUT=dist/text-patch \
  STEPS='wait:1500;click:90,60;type:find_in_page;wait:400;drag:335,258,554,258;right-click:528,258;wait:200;shot:link-menu;click:850,450;click:540,195;type:needle;wait:350;drag:285,258,348,258;wait:100;shot:find'
```
