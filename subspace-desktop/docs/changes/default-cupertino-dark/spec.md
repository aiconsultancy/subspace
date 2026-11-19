# Spec: Cupertino Dark as the default theme

## Declaring the default

- `ProductDescriptor.default_theme` is the display name of one theme variant,
  such as `Cupertino Dark`. `ProductDescriptor.themes` still lists bundled
  theme document IDs (`one-dark`, `cupertino`, `solarized` and so on).
- Before this change the default was a document ID, and the app used that
  document's first variant. Cupertino lists Light before Dark, so a dark
  default would have needed the document reordered. Naming the variant states
  the choice directly, matches what `appearance.theme` stores and what
  Settings shows, and leaves the theme files unchanged.
- `subspace_product::DEFAULT_THEME` is `Cupertino Dark`, and
  `DEFAULT_THEME_DOCUMENT` is `cupertino`, the document that holds it. The
  Subspace App declares `Some(DEFAULT_THEME)`. A descriptor with `None` gets
  the framework default, and the `cupertino` document is added to its
  catalogue when missing.
- The `SUBSPACE_DEFAULT_THEME` build override also takes a variant display
  name, for example `Solarized Dark`.

## Validation

- The product crate does not embed theme files, so it checks only the name's
  shape. A blank or padded name fails with `UnknownTheme`, both in the
  descriptor and in the build override.
- At startup the host's `validate_product_themes` reads the declared bundled
  documents and requires one of them to contain the default variant. An
  unknown name stops the app with
  `product default theme is not in its theme catalogue: <name>`.

## Applying it

- `theme::initialize` and `selected_theme` use the saved theme when it is
  registered, and the product's default variant otherwise. The old
  `DEFAULT_THEME_NAME` fallback is removed: validation guarantees the
  default exists.
- Dashboard default tokens stay One Dark, and `one-dark.json` must still agree
  with them. The tokens are the values used before or without a theme; every
  theme, including Cupertino Dark, overrides them when it is applied.

## Evidence rule

`AGENTS.md` **UI change evidence**: capture in Cupertino Dark, the default
theme, with `appearance.theme` unset. Capture other themes only when the
change is about a theme, colour or contrast, and then show each affected
theme in its light and dark variants.
