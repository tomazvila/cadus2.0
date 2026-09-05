//! Part 5 of the `template` tests. The header of `template_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::template::shuffle;
use common::template::*;

/// The rejection loop of the bounded draw decides the stream.
///
/// A bound of `2**63 + 1` rejects about one raw value in two, so the stream a
/// seed gives depends on the threshold and on every redraw. A bound of `2**63`
/// has a threshold of zero, and its low words are zero or `2**63` only, so a
/// draw whose low word equals the threshold is accepted.
#[test]
fn the_bounded_draw_rejects_at_the_threshold_and_accepts_on_it() {
    let mut rng = rng_from_seed(1);
    let bound = (1_u64 << 63) + 1;
    let rejected: Vec<u64> = (0..6).map(|_| below(&mut rng, bound)).collect();
    assert_eq!(
        rejected,
        [
            3_712_275_015_481_296_600,
            741_408_853_161_625_397,
            2_616_583_866_949_690_866,
            6_554_222_466_203_455_532,
            4_252_787_763_281_118_475,
            6_517_456_707_042_423_940,
        ]
    );
    let mut rng = rng_from_seed(1);
    let accepted: Vec<u64> = (0..6).map(|_| below(&mut rng, 1_u64 << 63)).collect();
    assert_eq!(
        accepted,
        [
            3_712_275_015_481_296_600,
            741_408_853_161_625_397,
            5_502_296_491_135_566_642,
            2_022_912_202_629_187_233,
            2_616_583_866_949_690_866,
            6_554_222_466_203_455_532,
        ]
    );
}

/// The Fisher-Yates walk swaps every position from the last one down to the
/// second, each with a draw at or below it, so the seed fixes the permutation.
#[test]
fn the_shuffle_is_the_permutation_the_seed_names() {
    let mut rng = rng_from_seed(1);
    let mut items: Vec<u8> = (0..8).collect();
    shuffle(&mut items, &mut rng);
    assert_eq!(items, [5, 6, 2, 4, 1, 7, 0, 3]);
    let mut one = vec![9_u8];
    shuffle(&mut one, &mut rng_from_seed(1));
    assert_eq!(one, [9]);
}
