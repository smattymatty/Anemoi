//! The world's own seeded RNG: PCG-XSH-RR 64/32 (O'Neill's `pcg32`).

const MULTIPLIER: u64 = 6364136223846793005;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// `pcg32_srandom_r(seed, stream)`: the same pair gives the same numbers anywhere.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform in `0..bound`, without modulo bias. `bound` must be above 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0, "Pcg32::below(0)");
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference `pcg32-demo` output for seed 42, stream 54.
    #[test]
    fn matches_the_reference_sequence() {
        let mut rng = Pcg32::new(42, 54);
        let got: Vec<u32> = (0..6).map(|_| rng.next_u32()).collect();
        let want = [
            0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e,
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn seed_and_stream_each_change_the_sequence() {
        let first = |seed, stream| Pcg32::new(seed, stream).next_u32();
        assert_ne!(first(42, 54), first(43, 54));
        assert_ne!(first(42, 54), first(42, 55));
    }

    /// Bound 2^31 + 1 rejects every draw under 2^31 - 1, so the reference
    /// sequence's second value (0x7b47f409) is skipped, not folded in.
    #[test]
    fn below_rejects_the_biased_draws() {
        let mut rng = Pcg32::new(42, 54);
        let bound = 0x8000_0001;
        assert_eq!(rng.below(bound), 0xa15c02b7 - bound);
        assert_eq!(rng.below(bound), 0xba1d3330 - bound, "0x7b47f409 rejected");
    }

    #[test]
    fn below_stays_in_range_and_reaches_every_value() {
        let mut rng = Pcg32::new(7, 0);
        let mut seen = [false; 6];
        for _ in 0..200 {
            seen[rng.below(6) as usize] = true;
        }
        assert_eq!(seen, [true; 6]);
        assert_eq!(rng.below(1), 0);
    }
}
