// One run at a time of an async task that takes a flag. Calls made while a
// run is in flight share it; if any of them asked for the flag (a Retry
// during a background refresh), one more run with the flag follows, so the
// request is never dropped, unless `cancelled()` says the owner has gone:
// then no further run starts, so a queued rerun never issues I/O after close.

/**
 * @param {(flag: boolean) => Promise<void>} task
 * @param {{cancelled?: () => boolean}} [options]
 * @returns {(flag?: boolean) => Promise<void>}
 */
export function coalesce(task, { cancelled = () => false } = {}) {
  let running = null;
  let wanted = false;
  return function run(flag = false) {
    if (cancelled()) return Promise.resolve();
    if (running) {
      if (flag) wanted = true;
      return running;
    }
    running = (async () => {
      let next = flag;
      for (;;) {
        if (cancelled()) {
          wanted = false;
          break;
        }
        try {
          await task(next);
        } catch (error) {
          if (!wanted) throw error;
        }
        if (!wanted) break;
        wanted = false;
        next = true;
      }
    })().finally(() => {
      running = null;
    });
    return running;
  };
}
