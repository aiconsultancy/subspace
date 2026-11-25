//! Where the session starts: the scope's home, or the start the product's
//! route gives under it, which a view showing a page follows at once.

use super::{Effect, Here, Session};

pub(in super::super) const OUTSIDE: &str =
    "The product's start route is outside the session's scope.";

impl Session {
    /// The product's start route gave `route`: a reference under the home,
    /// or `None` for the home itself. When the start moves, a view showing
    /// a page goes there at once, and a session showing none, or one that
    /// ended, starts there rather than where it was. A route outside the
    /// scope is the product's mistake: the start is the home, and the Space
    /// logs it.
    pub(crate) fn route(&mut self, route: Option<&str>, here: Here) -> Vec<Effect> {
        let resolved = route.map(|route| self.scope.resolve(route));
        let mut effects = match resolved {
            Some(None) => vec![Effect::Log(OUTSIDE)],
            _ => Vec::new(),
        };
        let start = resolved
            .flatten()
            .unwrap_or_else(|| self.scope.home().to_owned());
        if start != self.start {
            self.start = start;
            effects.extend(self.restart(here));
        }
        effects
    }

    /// The start moved while the view was at `here`: a view showing a page
    /// loads the start, and nothing resumes where the view was. While the
    /// site refuses the session, the view loads it once a renewal succeeds
    /// instead, as the signal would send a load now back. Either way the
    /// session remembers the page the move left (`Session::left`).
    fn restart(&mut self, here: Here) -> Vec<Effect> {
        let showing = self.phase.renewing().is_some();
        let refused = self.phase.refused();
        if showing {
            self.left = here;
        }
        self.phase = self.take().restarted();
        match showing && !refused {
            true => vec![Effect::Load(self.start.clone())],
            false => Vec::new(),
        }
    }

    /// `page`, unless it is the page the start's last move left, which
    /// this forgets.
    pub(super) fn unless_left(&mut self, page: Here) -> Here {
        let left = self.left.take();
        page.filter(|page| left.as_ref() != Some(page))
    }
}
