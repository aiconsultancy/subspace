// One view's working set as a byte budget (SUB decision a17870f4): content
// read from the gateway, the formatted copies made from it and the content
// mounted on the page all count against it. Room is made before anything is
// read, formatted or mounted: a reservation frees the least recently used
// other entries until it fits. Nothing here reads or draws; the owner of
// each entry says how to free it. Shared by every view that pages content
// (officer activity, crew diffs), through `createPager` or directly.

/**
 * @param {number} limit  bytes the view may hold in all
 */
export function createBudget(limit) {
  /** key -> {bytes, used, evict} */
  const entries = new Map();
  let clock = 0;
  let total = 0;

  /** Free least recently used entries other than `keep` until `bytes` more fit; returns their eviction calls. */
  function makeRoom(bytes, keep) {
    const freed = [];
    while (total + bytes > limit) {
      let oldest = null;
      let oldestUsed = Infinity;
      for (const [key, entry] of entries) {
        if (key !== keep && entry.used < oldestUsed) {
          oldest = key;
          oldestUsed = entry.used;
        }
      }
      if (oldest === null) break;
      const entry = entries.get(oldest);
      entries.delete(oldest);
      total -= entry.bytes;
      if (entry.evict) freed.push(() => entry.evict(oldest));
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
    /** Bytes held now. At most `limit`, or the one forced entry alone when that is larger. */
    get used() {
      return total;
    },
    has: (key) => entries.has(key),
    bytes: (key) => entries.get(key)?.bytes ?? 0,
    /**
     * Hold `bytes` under `key`, replacing what it held, after freeing the
     * least recently used other entries until it fits. Each freed entry's
     * `evict(key)` is called once the reservation is in place, never in the
     * middle of it. Returns whether it fits within `limit`. When it does not,
     * nothing is held and nothing is freed, unless `force`: then it is held
     * anyway, alone, as a floor that must show (one part of an item, one
     * window of a turn).
     *
     * @param {string} key
     * @param {number} bytes
     * @param {((key: string) => void)|null} evict
     * @param {{force?: boolean}} [options]
     */
    reserve(key, bytes, evict, { force = false } = {}) {
      const fits = bytes <= limit;
      if (!fits && !force) return false;
      release(key);
      const freed = makeRoom(bytes, key);
      clock += 1;
      entries.set(key, { bytes, used: clock, evict });
      total += bytes;
      for (const call of freed) call();
      return fits;
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
