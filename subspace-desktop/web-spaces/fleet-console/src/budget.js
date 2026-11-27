// One view's working set as a byte budget (SUB decision a17870f4): content
// read from the gateway, the copies drawn from it and the content on the
// page all count against it. It is a cache budget only: nothing is hidden
// or cut to keep within it (SUB 20ce0349). Room is made before anything is
// read or drawn: a reservation frees the least recently used other entries
// until it fits. Nothing here reads or draws; the owner of
// each entry says how to free it. The console holds one, shared by every
// view that pages content (officer activity, crew diffs; SUB decision
// 75c3ed93 §3), each through its own `scope`, by `createPager` or directly.
//
// The budget alone decides what may take the total past the limit (SUB
// decision 1c19f413): the pinned entries (the activity view's inline
// snapshot) and the latest request (`request`), the one thing the person
// last asked for: a Show, a Load or another part. Neither is ever freed to
// make room for another entry; every other reservation fits beside them or
// is refused. A new request ends the last one's claim: it is freed like any
// other entry from then on.

/**
 * @param {number} limit  bytes the view may hold in all
 */
export function createBudget(limit) {
  /** key -> {bytes, used, evict, pinned} */
  const entries = new Map();
  let clock = 0;
  let total = 0;
  /** The key of the latest request, or null. */
  let requested = null;
  /** Whether `key` is never freed for another: pinned, or the latest request. */
  const kept = (key) => key === requested || entries.get(key)?.pinned === true;
  /** The bytes that can never be freed for `key`: the pinned entries and the latest request, other than `key` itself. */
  function fixedBeside(key) {
    let bytes = 0;
    for (const [other, entry] of entries) if (other !== key && kept(other)) bytes += entry.bytes;
    return bytes;
  }

  /**
   * An entry's place in the order entries are freed in, lowest first: least
   * recently used first, except that an entry that yields (a whole read
   * grown into free room) goes before any other.
   */
  const age = (entry) => (entry.yields ? entry.used - Number.MAX_SAFE_INTEGER : entry.used);

  /** The first entry in that order that may be freed and that `skip` does not name, or null. */
  function oldestKey(skip) {
    let oldest = null;
    let oldestUsed = Infinity;
    for (const [key, entry] of entries) {
      const used = age(entry);
      if (!kept(key) && !skip(key) && used < oldestUsed) {
        oldest = key;
        oldestUsed = used;
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

  /**
   * The entries to free, in `age` order and never a kept one or `key`, for
   * `bytes` under `key` (replacing what it holds) to fit: the one order both
   * `reserve` and `wouldFree` use.
   */
  function freeOrder(key, bytes) {
    let held = total - (entries.get(key)?.bytes ?? 0);
    const order = [];
    const others = [...entries].filter(([other]) => other !== key && !kept(other)).sort((a, b) => age(a[1]) - age(b[1]));
    for (const [other, entry] of others) {
      if (held + bytes <= limit) break;
      order.push(other);
      held -= entry.bytes;
    }
    return order;
  }

  /** Free least recently used entries, never a kept one or `keep`, until `bytes` more fit; returns their eviction calls. */
  const makeRoom = (bytes, keep) => freeOrder(keep, bytes).map(take);

  function release(key) {
    const entry = entries.get(key);
    if (!entry) return;
    entries.delete(key);
    total -= entry.bytes;
  }

  const budget = {
    limit,
    /** Bytes held now. At most `limit`, unless the pinned entries and the latest request (the floor) are larger. */
    get used() {
      return total;
    },
    /** The key of the latest request, or null. */
    get requested() {
      return requested;
    },
    has: (key) => entries.has(key),
    bytes: (key) => entries.get(key)?.bytes ?? 0,
    /**
     * `key` is what the person asked for last (a Show, a Load, another
     * part): with the pinned entries it is the floor, held even past the
     * limit and never freed for another. The request before it loses that
     * claim. Marks `key` as just used.
     *
     * @param {string} key
     */
    request(key) {
      requested = key;
      const entry = entries.get(key);
      if (entry) entry.used = ++clock;
    },
    /** Whether `bytes` under `key` fit within `limit` beside what is never freed for it. */
    fits: (key, bytes) => fixedBeside(key) + bytes <= limit,
    /**
     * The keys that holding `bytes` under `key` would free, least recently
     * used first, freeing nothing: so an owner can let go of an entry in its
     * own way before `reserve` would evict it.
     *
     * @param {string} key
     * @param {number} bytes
     * @returns {string[]}
     */
    wouldFree: (key, bytes) => freeOrder(key, bytes),
    /**
     * Hold `bytes` under `key`, replacing what it held, after freeing the
     * least recently used other entries until it fits. Each freed entry's
     * `evict(key)` is called once the reservation is in place, never in the
     * middle of it. It is held when it fits within `limit` beside the
     * pinned entries and the latest request, or when it is one of them
     * (`pinned`, or `key` is the latest request): the floor, held anyway.
     * Otherwise nothing is held, nothing is freed, and what `key` held
     * stays. Returns whether it is held. `makeRoom: false` frees nothing:
     * for a caller that fitted `bytes` to the room itself (the panel's
     * drawn bodies, a whole read grown into free room). `yields`: freed
     * before every other entry whenever room is needed (a whole read that
     * grew into free room gives it back first, SUB decisions feb0d839 and
     * 75c3ed93 §3).
     *
     * @param {string} key
     * @param {number} bytes
     * @param {((key: string) => void)|null} evict
     * @param {{pinned?: boolean, makeRoom?: boolean, yields?: boolean}} [options]
     */
    reserve(key, bytes, evict, { pinned = false, makeRoom: free = true, yields = false } = {}) {
      if (!pinned && key !== requested && fixedBeside(key) + bytes > limit) return false;
      release(key);
      const freed = free ? makeRoom(bytes, key) : [];
      clock += 1;
      entries.set(key, { bytes, used: clock, evict, pinned, yields });
      total += bytes;
      for (const call of freed) call();
      return true;
    },
    /**
     * Free the least recently used entry, never a pinned one or the latest
     * request, that `skip(key)` does not protect, calling its `evict`.
     * Returns whether one was freed.
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
      requested = null;
    },
    /**
     * This budget as one view sees it (SUB decision 75c3ed93 §3): the same
     * limit, room, order and latest request as every other view's, its keys
     * kept apart under `prefix`. Each view frees the others' least recently
     * used entries as it frees its own, and never the latest request,
     * whichever view made it. `wouldFree` names only this view's keys: the
     * others' entries are refitted or freed by their own views. `clear`
     * gives up this view's keys only, and its claim to the latest request.
     *
     * @param {string} prefix
     */
    scope(prefix) {
      const own = (key) => key.startsWith(prefix);
      const outer = (key) => prefix + key;
      const inner = (key) => key.slice(prefix.length);
      const evictOf = (evict) => (evict ? (key) => evict(inner(key)) : evict);
      return {
        limit,
        get used() {
          return total;
        },
        get requested() {
          return requested !== null && own(requested) ? inner(requested) : null;
        },
        has: (key) => budget.has(outer(key)),
        bytes: (key) => budget.bytes(outer(key)),
        request: (key) => budget.request(outer(key)),
        fits: (key, bytes) => budget.fits(outer(key), bytes),
        wouldFree: (key, bytes) => budget.wouldFree(outer(key), bytes).filter(own).map(inner),
        reserve: (key, bytes, evict, options) => budget.reserve(outer(key), bytes, evictOf(evict), options),
        freeOldest: (skip) => budget.freeOldest((key) => own(key) && skip(inner(key))),
        touch: (key) => budget.touch(outer(key)),
        release: (key) => release(outer(key)),
        releaseWhere: (match) => budget.releaseWhere((key) => own(key) && match(inner(key))),
        clear() {
          budget.releaseWhere(own);
          if (requested !== null && own(requested)) requested = null;
        },
      };
    },
  };
  return budget;
}
