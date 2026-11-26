// One view's working set as a byte budget (SUB decision a17870f4): content
// read from the gateway, the copies drawn from it and the content on the
// page all count against it. It is a cache budget only: nothing is hidden
// or cut to keep within it (SUB 20ce0349). Room is made before anything is
// read or drawn: a reservation frees the least recently used other entries
// until it fits. Nothing here reads or draws; the owner of
// each entry says how to free it. Shared by every view that pages content
// (officer activity, crew diffs), through `createPager` or directly.
//
// A pinned entry is never freed to make room: it is the floor that must
// show (the activity view's inline snapshot). Only a pinned or forced entry
// may take the total past the limit.

/**
 * @param {number} limit  bytes the view may hold in all
 */
export function createBudget(limit) {
  /** key -> {bytes, used, evict, pinned} */
  const entries = new Map();
  let clock = 0;
  let total = 0;

  /** The least recently used entry that may be freed and that `skip` does not name, or null. */
  function oldestKey(skip) {
    let oldest = null;
    let oldestUsed = Infinity;
    for (const [key, entry] of entries) {
      if (!entry.pinned && !skip(key) && entry.used < oldestUsed) {
        oldest = key;
        oldestUsed = entry.used;
      }
    }
    return oldest;
  }

  /** Remove `key` and return its eviction call. */
  function take(key) {
    const entry = entries.get(key);
    entries.delete(key);
    total -= entry.bytes;
    return () => entry.evict?.(key);
  }

  /** Free least recently used unpinned entries other than `keep` until `bytes` more fit; returns their eviction calls. */
  function makeRoom(bytes, keep) {
    const freed = [];
    while (total + bytes > limit) {
      const oldest = oldestKey((key) => key === keep);
      if (oldest === null) break;
      freed.push(take(oldest));
    }
    return freed;
  }

  function release(key) {
    const entry = entries.get(key);
    if (!entry) return;
    entries.delete(key);
    total -= entry.bytes;
  }

  return {
    limit,
    /** Bytes held now. At most `limit`, unless pinned and forced entries (the floor) are larger. */
    get used() {
      return total;
    },
    has: (key) => entries.has(key),
    bytes: (key) => entries.get(key)?.bytes ?? 0,
    /**
     * Hold `bytes` under `key`, replacing what it held, after freeing the
     * least recently used other entries until it fits. Each freed entry's
     * `evict(key)` is called once the reservation is in place, never in the
     * middle of it. Returns whether it fits within `limit` beside the pinned
     * entries, which are never freed. When it does not,
     * nothing is held and nothing is freed, unless `force`: then it is held
     * anyway, as a floor that must show (one part of an item). `pinned`
     * holds it so it is never freed for another (the inline snapshot).
     * `makeRoom: false` frees nothing: for a caller that fitted `bytes` to
     * the room itself (the panel's drawn bodies), so only its floor can
     * take the total past the limit.
     *
     * @param {string} key
     * @param {number} bytes
     * @param {((key: string) => void)|null} evict
     * @param {{force?: boolean, pinned?: boolean, makeRoom?: boolean}} [options]
     */
    reserve(key, bytes, evict, { force = false, pinned = false, makeRoom: free = true } = {}) {
      // It fits only beside what can never be freed: the pinned entries.
      let floor = 0;
      for (const [other, entry] of entries) if (entry.pinned && other !== key) floor += entry.bytes;
      const fits = floor + bytes <= limit;
      if (!fits && !force) return false;
      release(key);
      const freed = free ? makeRoom(bytes, key) : [];
      clock += 1;
      entries.set(key, { bytes, used: clock, evict, pinned });
      total += bytes;
      for (const call of freed) call();
      return fits;
    },
    /**
     * Free the least recently used unpinned entry that `skip(key)` does not
     * protect, calling its `evict`. Returns whether one was freed.
     *
     * @param {(key: string) => boolean} skip
     */
    freeOldest(skip) {
      const oldest = oldestKey(skip);
      if (oldest === null) return false;
      take(oldest)();
      return true;
    },
    /** Mark `key` as just used (opened), so it is freed last. */
    touch(key) {
      const entry = entries.get(key);
      if (entry) entry.used = ++clock;
    },
    /** Give up `key` without calling its `evict`. */
    release,
    /** Give up every key that `match` accepts, without calling `evict`. */
    releaseWhere(match) {
      for (const key of [...entries.keys()]) if (match(key)) release(key);
    },
    clear() {
      entries.clear();
      total = 0;
    },
  };
}
