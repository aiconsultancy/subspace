//! What the board knows about its page, from the page's events.

use crate::page::PageEvent;
use crate::policy::{blocked_message, browser_url, location, page_title};

const NO_DOWNLOADS: &str = "Downloads are not available in web Spaces yet. Use Open in browser.";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PageState {
    pub loading: bool,
    /// The address of the page the view shows.
    pub url: Option<String>,
    pub title: Option<String>,
    /// Why the board refused something, until the next page loads.
    pub notice: Option<String>,
}

impl PageState {
    /// Record `event`. Returns a page to open in the browser, if any.
    pub(crate) fn apply(&mut self, event: PageEvent) -> Option<String> {
        match event {
            PageEvent::Loading(url) => {
                self.loading = true;
                self.url = Some(url);
            }
            PageEvent::Loaded(url) => {
                self.loading = false;
                self.url = Some(url);
                self.notice = None;
            }
            PageEvent::Title(raw) => self.title = page_title(&raw),
            PageEvent::Blocked(url) => self.notice = Some(blocked_message(&url)),
            PageEvent::NewWindow(url) => {
                let target = browser_url(&url);
                if target.is_none() {
                    self.notice = Some(blocked_message(&url));
                }
                return target;
            }
            PageEvent::Download => self.notice = Some(NO_DOWNLOADS.into()),
            PageEvent::Message { .. } => {}
            #[cfg(target_os = "macos")]
            PageEvent::SessionRequired => {}
        }
        None
    }

    /// Events were dropped: stop showing a load that may have finished and
    /// take the address from the view itself.
    pub(crate) fn resync(&mut self, url: Option<String>) {
        self.loading = false;
        if url.is_some() {
            self.url = url;
        }
    }

    /// The status bar text: a notice, else where the page is.
    pub(crate) fn status(&self) -> Option<String> {
        self.notice
            .clone()
            .or_else(|| self.url.as_deref().and_then(location))
    }
}

#[cfg(test)]
mod tests {
    use super::PageState;
    use crate::page::PageEvent;

    #[test]
    fn loads_track_the_address_and_clear_notices() {
        let mut state = PageState::default();
        assert_eq!(
            state.apply(PageEvent::Loading("https://a.example/x".into())),
            None
        );
        assert!(state.loading);
        state.apply(PageEvent::Blocked("mailto:a@b.c".into()));
        assert!(state.status().unwrap().contains("mailto:"));
        state.apply(PageEvent::Loaded("https://b.example/y".into()));
        assert!(!state.loading);
        assert_eq!(state.status().as_deref(), Some("b.example"));
    }

    #[test]
    fn new_windows_open_web_pages_in_the_browser_only() {
        let mut state = PageState::default();
        assert_eq!(
            state
                .apply(PageEvent::NewWindow("https://docs.example/".into()))
                .as_deref(),
            Some("https://docs.example/")
        );
        assert_eq!(state.notice, None);
        assert_eq!(
            state.apply(PageEvent::NewWindow("zoommtg://join".into())),
            None
        );
        assert!(state.notice.is_some());
    }

    #[test]
    fn titles_downloads_and_dropped_events_update_the_state() {
        let mut state = PageState::default();
        state.apply(PageEvent::Title(" Home \n".into()));
        assert_eq!(state.title.as_deref(), Some("Home"));
        state.apply(PageEvent::Download);
        assert!(state.status().unwrap().contains("Downloads"));
        state.apply(PageEvent::Loading("https://a.example/".into()));
        state.resync(None);
        assert!(!state.loading);
        assert_eq!(state.url.as_deref(), Some("https://a.example/"));
        state.resync(Some("https://b.example/".into()));
        assert_eq!(state.url.as_deref(), Some("https://b.example/"));
    }
}
