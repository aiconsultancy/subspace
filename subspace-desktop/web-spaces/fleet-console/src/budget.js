// One view's working set as a byte budget (SUB decision a17870f4): content
// read from the gateway, the copies drawn from it and the content on the
// page all count against it. It is a cache budget only: nothing is hidden
// or cut to keep within it (SUB 20ce0349). Room is made before anything is
// read or drawn: a reservation frees the least recently used other entries
// until it fits. Nothing here reads or draws; the owner of
// each entry says how to free it. Shared by every view that pages content
// (officer activity, crew diffs), through `createPager` or directly.
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
  /** No entry held at another size. */
  const AS_HELD = new Map();
  /** The bytes that can never be freed for `key`: the pinned entries and the latest request, other than `key` itself. */
  function fixedBeside(key) {
    let bytes = 0;
    for (const [other, entry] of entries) if (other !== key && kept(other)) bytes += entry.bytes;
    return bytes;
  }
  /** The floor a reservation under `key` must fit beside: null when `key` is part of it (`pinned`, or the latest request). */
  const floorFor = (key, pinned) => (pinned || key === requested ? null : fixedBeside(key));
  /**
   * Whether `bytes` under `key` may be held: they fit beside what is never
   * freed for it, or `key` is part of that floor. The rule `reserve` and
   * every plan (`wouldFree`) use.
   */
  function admits(key, bytes, pinned) {
    const floor = floorFor(key, pinned);
    return floor === null || floor + bytes <= limit;
  }

  /** The least recently used entry that may be freed and that `skip` does not name, or null. */
  function oldestKey(skip) {
    let oldest = null;
    let oldestUsed = Infinity;
    for (const [key, entry] of entries) {
      if (!kept(key) && !skip(key) && entry.used < oldestUsed) {
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

  /**
   * A reservation of `bytes` under `key` (replacing what it holds), weighed
   * against changes to other entries before any is made: the one order both
   * `reserve` and `wouldFree` use. The entries that may be freed are sorted
   * by use once, and what is held is counted once; `set` and `delete` then
   * resize an entry, as if reserved just now, after every other and in the
   * order set, and `freed` walks that order without sorting again (SUB
   * decisions 8346cf82, d43de89e). It reads the entries as they are when
   * made, so it is used before the budget changes.
   */
  function plan(key, bytes, pinned) {
    const sizes = new Map();
    const counts = (other) => other !== key && entries.has(other);
    const change = (other, size) => (counts(other) ? size - entries.get(other).bytes : 0);
    // What stays held beside `key`, and the part of it never freed for `key` (null when `key` is part of that floor).
    let held = total - (entries.get(key)?.bytes ?? 0);
    let fixed = floorFor(key, pinned);
    const byUse = [...entries.keys()].filter((other) => other !== key && !kept(other)).sort((a, b) => entries.get(a).used - entries.get(b).used);
    /** Count `bytesMore` more bytes (fewer when negative) under `other`. */
    const add = (other, bytesMore) => {
      held += bytesMore;
      if (fixed !== null && kept(other)) fixed += bytesMore;
    };
    return {
      /** Weigh `other` as holding `size` bytes. One set again keeps its place in the order. */
      set(other, size) {
        add(other, change(other, size) - (sizes.has(other) ? change(other, sizes.get(other)) : 0));
        sizes.set(other, size);
      },
      delete(other) {
        if (!sizes.has(other)) return;
        add(other, -change(other, sizes.get(other)));
        sizes.delete(other);
      },
      has: (other) => sizes.has(other),
      /** The entries resized, in the order set. */
      keys: () => [...sizes.keys()],
      /**
       * The keys the reservation would free, least recently used first; none
       * when `reserve` would refuse it. `without` weighs one resized entry as
       * it holds now, in its own place in the order, without changing the plan.
       */
      freed(without = null) {
        const undo = sizes.has(without) ? change(without, sizes.get(without)) : 0;
        if (fixed !== null && fixed - (without !== null && kept(without) ? undo : 0) + bytes > limit) return [];
        let room = held - undo;
        const holds = (other) => (other !== without && sizes.has(other) ? sizes.get(other) : entries.get(other).bytes);
        const order = [];
        /** Free `other` unless `bytes` already fit; returns whether it was needed. */
        const free = (other) => {
          if (room + bytes <= limit) return false;
          order.push(other);
          room -= holds(other);
          return true;
        };
        // Those held as now, by when they were used, then the resized ones, in the order set.
        for (const other of byUse) if ((other === without || !sizes.has(other)) && !free(other)) return order;
        for (const other of sizes.keys()) if (other !== without && counts(other) && !kept(other) && !free(other)) break;
        return order;
      },
    };
  }

  /** Free least recently used entries, never a kept one or `keep`, until `bytes` more fit; returns their eviction calls. */
  const makeRoom = (bytes, keep, pinned) => plan(keep, bytes, pinned).freed().map(take);

  function release(key) {
    const entry = entries.get(key);
    if (!entry) return;
    entries.delete(key);
    total -= entry.bytes;
  }

  return {
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
     * own way before `reserve` would evict it. None when `reserve` would
     * refuse it, as it then frees nothing (SUB decision cf21d510); `pinned`
     * as `reserve` takes it. `as` (key -> bytes) asks as if those entries
     * held those bytes instead, each reserved just now in that order: so an
     * owner can plan several changes before making any (SUB decision
     * 8346cf82).
     *
     * @param {string} key
     * @param {number} bytes
     * @param {{pinned?: boolean, as?: Map<string, number>}} [options]
     * @returns {string[]}
     */
    wouldFree(key, bytes, { pinned = false, as = AS_HELD } = {}) {
      const weighed = plan(key, bytes, pinned);
      for (const [other, size] of as) weighed.set(other, size);
      return weighed.freed();
    },
    /**
     * `wouldFree` for an owner weighing many plans of one reservation, the
     * order made once: `set(key, bytes)` and `delete(key)` change the plan,
     * `freed(without)` names what it would free, as `wouldFree` with `as`
     * (or with `without` left out of it), and `keys()` lists what is
     * resized, in order. Nothing changes in the budget. It answers for the
     * entries as they were when it was made: use it before the budget
     * changes (SUB decision d43de89e).
     *
     * @param {string} key
     * @param {number} bytes
     * @param {{pinned?: boolean}} [options]
     */
    plan: (key, bytes, { pinned = false } = {}) => plan(key, bytes, pinned),
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
     * drawn bodies).
     *
     * @param {string} key
     * @param {number} bytes
     * @param {((key: string) => void)|null} evict
     * @param {{pinned?: boolean, makeRoom?: boolean}} [options]
     */
    reserve(key, bytes, evict, { pinned = false, makeRoom: free = true } = {}) {
      if (!admits(key, bytes, pinned)) return false;
      release(key);
      const freed = free ? makeRoom(bytes, key, pinned) : [];
      clock += 1;
      entries.set(key, { bytes, used: clock, evict, pinned });
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
    /**
     * Lower what `key` holds to `bytes`, never raising it: it only gives
     * room back, so it needs no admission and cannot be refused, and
     * `key` keeps its place in the freeing order (SUB decision 8346cf82).
     * Returns whether `key` is held.
     *
     * @param {string} key
     * @param {number} bytes
     */
    shrink(key, bytes) {
      const entry = entries.get(key);
      if (!entry) return false;
      if (bytes < entry.bytes) {
        total -= entry.bytes - bytes;
        entry.bytes = bytes;
      }
      return true;
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
  };
}
