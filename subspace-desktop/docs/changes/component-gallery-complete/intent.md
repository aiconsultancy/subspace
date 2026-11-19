# Intent: a complete component gallery and clear component rules

The owner asked:

> Can we make sure the rules and guidance about using the components and
> component library are clear. In short always prefer GPUI-kit components
> first, build new components from GPUI-kit components first, go custom
> second. Everything should be a component and there should be an
> interactive example on the component board. Every part of the UI should be
> represented there with easy access to the component's name/id for
> reference that can be copy pasted. I also think the component board should
> have every GPUI-Kit component on it, maybe with a toggle to hide unused
> components.

## Why

The component guidance was spread over several AGENTS.md bullets and the
catalogue's ownership section, and one bullet limited direct GPUI Kit use to
layout and implementation, which contradicts "GPUI Kit first". The gallery
showed most shared controls but not the messaging, navigation and form
helpers, no shell surfaces, and only three GPUI Kit components, so it could
not answer "what already exists?" for a person or an agent choosing a
component.

## Outcome

- One canonical, ordered rule set in `docs/ui-components.md`, summarised in
  AGENTS.md and checked in the pull request and board checklists.
- Tests that fail `make check` when the catalogue, the dashboard's public
  modules, the gallery and GPUI Kit's component list drift apart.
- A gallery with every shared component, every shell surface and every GPUI
  Kit 0.9.1 component, each with a copyable name, navigable by section,
  filter and `/` search, with a toggle that hides the GPUI Kit components
  Subspace does not use.
