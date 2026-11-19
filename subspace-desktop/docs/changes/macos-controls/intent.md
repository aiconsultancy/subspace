# Intent: macOS controls and a complete theme designer

The owner asked to style buttons from the theme: each kind of button should
have a corner radius, padding, size and text size that a theme sets. Text
sizes were inconsistent because some buttons used GPUI Kit's rem-relative
`text_xs`/`text_sm` helpers and others absolute pixels, so the same visual
level showed different sizes.

> Can we work on styling of the buttons? I think it's part of the theme we
> should be able to set corner radius for the different types of buttons, and
> set the padding and size. […] I noticed this inconsistent font sizing.

The macOS-styled themes should follow Apple's current guidelines:

> I like the Cupertino and Subspace font themes to have buttons which are more
> in the idiomatic style of macOS 27. […] So I guess that's the liquid glass
> style.

> To start with, let's just try and make the Cupertino light and dark and
> Subspace themes in keeping with macOS style guidelines around buttons and
> text spacing and such like.

The theme designer must preview every edit live, including in the Component
gallery, and later cover the whole theme:

> The theme designer should update the component view in real time.

> And we should add way more controls to the theme designer.

The work is two changes:

1. **macOS controls**: per-role button tokens, one sizing model, Liquid Glass
   toolbar buttons and macOS values for Cupertino Light, Cupertino Dark and
   Subspace One Dark, with live preview of the new tokens.
2. **Theme designer**: sections with native disclosure, every token the app
   reads, per-section reset, and live preview of each edit.
