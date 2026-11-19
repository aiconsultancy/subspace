//! The gallery catalogue: sections in display order, each with its items.
//!
//! An item name is the reference to copy into a request or into code: a
//! shared component relative to `subspace_dashboard` (`action::icon_button`),
//! a shell surface (`shell::status_bar`) or a full GPUI Kit path
//! (`gpui_kit::component::button::Button`). `docs/ui-components.md` lists
//! the same names.

use gpui_kit::{Context, Window};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::kit::{advanced, basic, form, layout};
use crate::samples::{
    Sample, buttons, choices, content, dialogs, feedback, inconsistencies, inputs, lists, menus,
    messaging, navigation, shell,
};

/// Builds the samples of one item, one for each state it shows.
pub(crate) type SampleFn =
    fn(&ComponentGallery, &Tokens, &mut Window, &mut Context<ComponentGallery>) -> Vec<Sample>;

/// Where a section's items come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Library {
    /// Shared Subspace components in `framework/board-kit/dashboard`.
    Subspace,
    /// Surfaces the host shell composes from shared components.
    Shell,
    /// GPUI Kit components, grouped as GPUI Kit's documentation groups them.
    Kit,
}

impl Library {
    pub(crate) const ALL: [Self; 3] = [Self::Subspace, Self::Shell, Self::Kit];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Subspace => "Subspace components",
            Self::Shell => "Shell",
            Self::Kit => "GPUI Kit",
        }
    }
}

/// What the gallery knows about a GPUI Kit component.
pub(crate) struct Kit {
    /// Paths below `gpui_kit::component`, such as `button::Button`, or text
    /// containing `(`, such as `.open_sheet(`. Code outside the gallery that
    /// imports or names one of the paths, or contains the text, uses the
    /// component; a test checks [`Kit::used`] against the source.
    pub(crate) paths: &'static [&'static str],
    /// Whether code outside the gallery uses the component.
    pub(crate) used: bool,
    /// The gallery name of the shared Subspace treatment to use instead of
    /// the bare component, when there is one.
    pub(crate) prefer: Option<&'static str>,
}

/// One gallery card.
pub(crate) struct Item {
    /// The reference to copy, such as `action::icon_button`. Unique in the gallery.
    pub(crate) name: &'static str,
    /// The file that owns the component, relative to the project root, or
    /// the GPUI Kit source file.
    pub(crate) source: &'static str,
    /// Where Subspace uses it, or how a one-off differs from a shared component.
    pub(crate) used_in: &'static str,
    pub(crate) samples: SampleFn,
    /// Set for GPUI Kit components only.
    pub(crate) kit: Option<Kit>,
}

impl Item {
    /// True for a GPUI Kit component that code outside the gallery does not use.
    pub(crate) fn is_unused_kit(&self) -> bool {
        self.kit.as_ref().is_some_and(|kit| !kit.used)
    }
}

/// One gallery section and its items, in display order.
pub(crate) struct Section {
    /// A stable identifier for the heading element and the stored choice.
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) library: Library,
    pub(crate) items: &'static [Item],
}

pub(crate) static SECTIONS: [Section; 16] = [
    Section {
        id: "buttons",
        title: "Buttons",
        library: Library::Subspace,
        items: buttons::ITEMS,
    },
    Section {
        id: "choices",
        title: "Choices",
        library: Library::Subspace,
        items: choices::ITEMS,
    },
    Section {
        id: "inputs",
        title: "Inputs",
        library: Library::Subspace,
        items: inputs::ITEMS,
    },
    Section {
        id: "menus",
        title: "Menus",
        library: Library::Subspace,
        items: menus::ITEMS,
    },
    Section {
        id: "dialogs",
        title: "Dialogs",
        library: Library::Subspace,
        items: dialogs::ITEMS,
    },
    Section {
        id: "feedback",
        title: "Feedback",
        library: Library::Subspace,
        items: feedback::ITEMS,
    },
    Section {
        id: "content",
        title: "Content",
        library: Library::Subspace,
        items: content::ITEMS,
    },
    Section {
        id: "messaging",
        title: "Messaging",
        library: Library::Subspace,
        items: messaging::ITEMS,
    },
    Section {
        id: "navigation",
        title: "Navigation and chrome",
        library: Library::Subspace,
        items: navigation::ITEMS,
    },
    Section {
        id: "lists-and-tables",
        title: "Lists and tables",
        library: Library::Subspace,
        items: lists::ITEMS,
    },
    Section {
        id: "inconsistencies",
        title: "Inconsistencies (one-offs)",
        library: Library::Subspace,
        items: inconsistencies::ITEMS,
    },
    Section {
        id: "shell",
        title: "Shell surfaces",
        library: Library::Shell,
        items: shell::ITEMS,
    },
    Section {
        id: "kit-basic",
        title: "Basic components",
        library: Library::Kit,
        items: basic::ITEMS,
    },
    Section {
        id: "kit-form",
        title: "Form components",
        library: Library::Kit,
        items: form::ITEMS,
    },
    Section {
        id: "kit-layout",
        title: "Layout components",
        library: Library::Kit,
        items: layout::ITEMS,
    },
    Section {
        id: "kit-advanced",
        title: "Advanced components",
        library: Library::Kit,
        items: advanced::ITEMS,
    },
];

/// Every item in display order.
pub(crate) fn items() -> impl Iterator<Item = &'static Item> {
    SECTIONS.iter().flat_map(|section| section.items)
}

/// The section and item called `name`.
pub(crate) fn find(name: &str) -> Option<(&'static Section, &'static Item)> {
    SECTIONS.iter().find_map(|section| {
        section
            .items
            .iter()
            .find(|item| item.name == name)
            .map(|item| (section, item))
    })
}
