//! Lifecycle polling for board-owned retained-content permits.
use gpui_kit::{Context, Task};
use std::time::Duration;

const PERMIT_POLL: Duration = Duration::from_millis(250);

/// Poll until the board has no retained entries or its entity is dropped.
/// Keep the returned task in the board; replacing it cancels the previous watcher.
/// The callback must only inspect in-memory permits and update board-owned state.
// qual:api
pub fn watch_retained<T: 'static>(
    cx: &mut Context<T>,
    poll: fn(&mut T, &mut Context<T>) -> bool,
) -> Task<()> {
    cx.spawn(async move |board, cx| {
        loop {
            cx.background_executor().timer(PERMIT_POLL).await;
            if !board.update(cx, poll).unwrap_or(false) {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{PERMIT_POLL, watch_retained};
    use gpui_kit::{AppContext as _, Context, TestAppContext};

    fn poll_once(calls: &mut usize, _: &mut Context<usize>) -> bool {
        *calls += 1;
        false
    }

    #[gpui_kit::test]
    fn empty_board_stops_polling_and_replacing_a_task_cancels_the_old_watcher(
        cx: &mut TestAppContext,
    ) {
        let board = cx.new(|_| 0_usize);
        let mut task = board.update(cx, |_, cx| watch_retained(cx, poll_once));
        cx.run_until_parked();
        task = board.update(cx, |_, cx| {
            drop(task);
            watch_retained(cx, poll_once)
        });
        cx.run_until_parked();
        cx.background_executor.advance_clock(PERMIT_POLL);
        cx.run_until_parked();
        assert_eq!(board.read_with(cx, |calls, _| *calls), 1);
        cx.background_executor.advance_clock(PERMIT_POLL);
        cx.run_until_parked();
        assert_eq!(board.read_with(cx, |calls, _| *calls), 1);
        drop(task);
    }
}
