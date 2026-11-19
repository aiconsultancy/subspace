//! What Copilot is said to be doing while a turn runs. The words are the
//! only playful thing in the panel, so they stay work-safe and lower case,
//! and they never repeat twice in a row.

/// How long one phrase stays on screen.
pub(super) const PHRASE_MS: u128 = 2_500;
/// The step between phrases. It shares no factor with [`WORDS`]'s length, so
/// stepping by it visits every phrase before any of them comes round again,
/// and never lands on the same one twice running.
const STEP: usize = 73;

/// Which phrase a turn shows at `tick`. The seed is the turn, so two turns
/// in a row open with different words.
pub(super) fn index(seed: u64, tick: u64) -> usize {
    let seed = (seed % WORDS.len() as u64) as usize;
    let steps = (tick % WORDS.len() as u64) as usize * STEP;
    (seed + steps) % WORDS.len()
}

/// The phrase at `index`, which the status line reads.
pub(super) fn phrase(index: usize) -> &'static str {
    WORDS[index % WORDS.len()]
}

/// How many phrases a turn has shown after running for `elapsed`.
pub(super) fn ticks(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_millis() / PHRASE_MS).unwrap_or_default()
}

/// Two hundred things a chat assistant might plausibly be up to.
pub(super) const WORDS: [&str; 200] = [
    "thinking",
    "considering",
    "pondering",
    "deliberating",
    "ruminating",
    "mulling it over",
    "musing",
    "reflecting",
    "contemplating",
    "cogitating",
    "plotting",
    "scheming",
    "strategising",
    "planning",
    "mapping it out",
    "charting a course",
    "drafting",
    "sketching",
    "outlining",
    "blueprinting",
    "discombobulating",
    "bamboozling the compiler",
    "untangling",
    "unknotting",
    "unscrambling",
    "disentangling",
    "decluttering",
    "defragmenting",
    "debugging",
    "rubber-ducking",
    "head-scratching",
    "chin-stroking",
    "brow-furrowing",
    "nose-tapping",
    "eyebrow-raising",
    "squinting",
    "blinking slowly",
    "staring into the middle distance",
    "gazing at the ceiling",
    "counting ceiling tiles",
    "squeezing",
    "wringing out the details",
    "distilling",
    "percolating",
    "brewing",
    "steeping",
    "marinating",
    "simmering",
    "reducing the sauce",
    "letting it rest",
    "pontificating",
    "expounding",
    "elaborating",
    "waxing lyrical",
    "holding forth",
    "soliloquising",
    "monologuing",
    "narrating",
    "annotating",
    "footnoting",
    "exercising",
    "stretching",
    "warming up",
    "limbering up",
    "doing lunges",
    "jogging on the spot",
    "shadow-boxing",
    "flexing",
    "cracking knuckles",
    "touching toes",
    "rummaging",
    "foraging",
    "scavenging",
    "spelunking",
    "excavating",
    "prospecting",
    "panning for gold",
    "metal-detecting",
    "beachcombing",
    "truffle-hunting",
    "consulting the archives",
    "leafing through notes",
    "thumbing the index",
    "checking the appendix",
    "cross-referencing",
    "fact-checking",
    "double-checking",
    "triple-checking",
    "proofreading",
    "peer-reviewing",
    "assembling",
    "soldering",
    "welding",
    "bolting things together",
    "gluing",
    "taping",
    "stapling",
    "clamping",
    "screwing it in",
    "hammering away",
    "calibrating",
    "tuning",
    "aligning",
    "levelling",
    "balancing",
    "adjusting the knobs",
    "turning dials",
    "flipping switches",
    "pulling levers",
    "pressing buttons",
    "measuring twice",
    "cutting once",
    "eyeballing it",
    "lining things up",
    "squaring the corners",
    "plumbing the depths",
    "sounding it out",
    "taking soundings",
    "triangulating",
    "surveying",
    "computing",
    "calculating",
    "crunching numbers",
    "tallying",
    "counting on fingers",
    "doing long division",
    "carrying the one",
    "rounding up",
    "estimating",
    "approximating",
    "shuffling",
    "sorting",
    "filing",
    "stacking",
    "nesting",
    "bundling",
    "batching",
    "queueing politely",
    "forming an orderly line",
    "taking a ticket",
    "juggling",
    "spinning plates",
    "balancing on one leg",
    "walking the tightrope",
    "catching falling knives",
    "herding cats",
    "wrangling",
    "corralling",
    "rounding up strays",
    "chasing tails",
    "polishing",
    "buffing",
    "sanding down",
    "smoothing the edges",
    "dusting off",
    "tidying up",
    "wiping the slate",
    "sweeping up",
    "straightening the picture",
    "fluffing the cushions",
    "negotiating",
    "bargaining",
    "haggling",
    "bartering",
    "compromising",
    "conferring",
    "caucusing",
    "deliberating in committee",
    "forming a working group",
    "writing a memo",
    "daydreaming",
    "woolgathering",
    "noodling",
    "doodling",
    "tinkering",
    "fiddling",
    "pottering about",
    "puttering",
    "faffing productively",
    "whittling",
    "brainstorming",
    "idea-wrangling",
    "connecting dots",
    "joining the dots",
    "following a hunch",
    "chasing a lead",
    "sniffing around",
    "poking about",
    "prodding gently",
    "having a look",
    "nearly there",
    "closing in",
    "rounding the corner",
    "on the home straight",
    "tying up loose ends",
    "crossing the t's",
    "dotting the i's",
    "putting the kettle on",
    "wrapping up",
    "finishing touches",
];

#[cfg(test)]
mod tests {
    use super::{PHRASE_MS, STEP, WORDS, index, phrase, ticks};
    use std::collections::HashSet;
    use std::time::Duration;

    #[test]
    fn there_are_two_hundred_distinct_phrases() {
        assert_eq!(WORDS.len(), 200);
        let distinct: HashSet<&&str> = WORDS.iter().collect();
        assert_eq!(distinct.len(), WORDS.len(), "every phrase is its own");
    }

    #[test]
    fn every_phrase_is_one_tidy_lower_case_line() {
        for word in WORDS {
            assert!(!word.trim().is_empty(), "an empty phrase");
            assert_eq!(word, word.trim(), "{word} has loose whitespace");
            assert_eq!(word, word.to_lowercase(), "{word} is not lower case");
            assert!(!word.contains('\n'), "{word} is more than one line");
            assert!(!word.contains("  "), "{word} has a double space");
        }
    }

    #[test]
    fn a_phrase_never_follows_itself() {
        for seed in [0, 1, 7, 199, 1_000] {
            for tick in 0..WORDS.len() as u64 * 2 {
                assert_ne!(
                    index(seed, tick),
                    index(seed, tick + 1),
                    "seed {seed} repeated at tick {tick}"
                );
            }
        }
    }

    #[test]
    fn a_turn_sees_every_phrase_before_one_comes_round_again() {
        let seen: HashSet<usize> = (0..WORDS.len() as u64).map(|tick| index(3, tick)).collect();
        assert_eq!(seen.len(), WORDS.len(), "the order covers the whole list");
        assert_eq!(index(3, 0), index(3, WORDS.len() as u64), "then it repeats");
    }

    #[test]
    fn each_turn_opens_on_a_different_phrase() {
        let openers: Vec<usize> = (0..5).map(|turn| index(turn, 0)).collect();
        let distinct: HashSet<&usize> = openers.iter().collect();
        assert_eq!(distinct.len(), openers.len(), "{openers:?}");
    }

    #[test]
    fn the_step_shares_no_factor_with_the_list() {
        let (mut a, mut b) = (STEP, WORDS.len());
        while b != 0 {
            (a, b) = (b, a % b);
        }
        assert_eq!(a, 1, "the step must be coprime with the list");
    }

    #[test]
    fn a_phrase_lasts_about_two_and_a_half_seconds() {
        assert_eq!(ticks(Duration::from_millis(0)), 0);
        assert_eq!(ticks(Duration::from_millis(PHRASE_MS as u64 - 1)), 0);
        assert_eq!(ticks(Duration::from_millis(PHRASE_MS as u64)), 1);
        assert_eq!(ticks(Duration::from_secs(10)), 4);
    }

    #[test]
    fn an_index_past_the_end_still_names_a_phrase() {
        assert_eq!(phrase(0), WORDS[0]);
        assert_eq!(phrase(WORDS.len()), WORDS[0]);
    }
}
