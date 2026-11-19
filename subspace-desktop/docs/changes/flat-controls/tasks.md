# Tasks: flat controls

- [x] Choice density tokens: stored by name, held as an index, rejected
      when unknown.
- [x] `--toolbar-button-rest` and `--toolbar-button-hover` (`flat` or
      `glass`, default `flat`) read into `Controls::toolbar_look`.
- [x] `button-hover` and `button-on` tones with mode defaults.
- [x] `IconButton::surface`, `icon_button::paint` and the flat and glass
      states; latched flat buttons keep their pressed state and answer the
      pointer.
- [x] `IconButton::flat()`; the status bar builds all four commands through
      one helper, and Report issue matches the version.
- [x] Theme designer: segmented choices for the two tokens, the two tones
      under Colours, live preview, Save and Reset.
- [x] Tests: tokens, surfaces, every look's states, the status bar in a
      glass theme, the designer, contrast in the Subspace themes.
- [x] Docs: appearance, UI components.
- [x] Before and after screenshots in Cupertino Dark and Light and Subspace
      One Dark, and a glass fixture theme.
