use std::thread::{self, JoinHandle};

use gpui_kit::{App, Global, Task, VisualTestContext};
use tokio::sync::oneshot;

use super::super::binary::ByteResult;
use super::super::pages::PageResult;
use super::super::read_executor::{ReadExecutor, ReadResult, ReadWork};

/// The test dispatcher runs blocking work on the UI thread. Opt in to a real
/// worker so the test can draw and interact while an HTTP response is gated.
#[derive(Default)]
pub(super) struct ThreadedReads(Vec<JoinHandle<DeliverRead>>);

type DeliverRead = Box<dyn FnOnce() + Send>;

impl Global for ThreadedReads {}

impl ThreadedReads {
    pub(super) fn install(window: &mut VisualTestContext) {
        window.update(|_, cx| {
            cx.set_global(Self::default());
            cx.set_global(ReadExecutor::<ReadResult>(Self::start));
            cx.set_global(ReadExecutor::<PageResult>(Self::start));
            cx.set_global(ReadExecutor::<ByteResult>(Self::start));
        });
    }

    fn start<T: Send + 'static>(cx: &mut App, work: ReadWork<T>) -> Task<T> {
        let (tx, rx) = oneshot::channel();
        cx.global_mut::<Self>()
            .0
            .push(thread::spawn(move || -> DeliverRead {
                let result = work();
                Box::new(move || {
                    _ = tx.send(result);
                })
            }));
        cx.spawn(async move |_| rx.await.expect("test worker delivers its result"))
    }

    pub(super) fn finish_last(window: &mut VisualTestContext) {
        let index = window.update(|_, cx| cx.global::<Self>().0.len() - 1);
        Self::finish(window, index);
    }

    pub(super) fn finish(window: &mut VisualTestContext, index: usize) {
        // Only the UI test thread can wake its deterministic scheduler.
        let deliver =
            window.update(|_, cx| cx.global_mut::<Self>().0.remove(index).join().unwrap());
        deliver();
        window.run_until_parked();
    }
}
