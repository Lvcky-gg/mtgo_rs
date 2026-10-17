//! Bounded combinations for modal choices; the engine validates each candidate.

use mtg_engine::choice::Answer;

pub(super) fn alternatives(available: usize, min: u8, max: u8) -> Vec<Answer> {
    // Answers encode mode indices as u8.
    let available = available.min(usize::from(u8::MAX) + 1);
    let mut out = Vec::new();
    for size in usize::from(min)..=usize::from(max).min(available) {
        let mut selected: Vec<usize> = (0..size).collect();
        loop {
            out.push(Answer::Modes(selected.iter().map(|i| *i as u8).collect()));
            if out.len() == 24 {
                return out;
            }
            let Some(pivot) = (0..size)
                .rev()
                .find(|i| selected[*i] < available - size + *i)
            else {
                break;
            };
            selected[pivot] += 1;
            for next in pivot + 1..size {
                selected[next] = selected[next - 1] + 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picks(available: usize, min: u8, max: u8) -> Vec<Vec<u8>> {
        alternatives(available, min, max)
            .into_iter()
            .map(|answer| {
                let Answer::Modes(modes) = answer else {
                    unreachable!()
                };
                modes
            })
            .collect()
    }

    #[test]
    fn choose_two_compares_every_pair_without_repeats() {
        assert_eq!(
            picks(4, 2, 2),
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
    }

    #[test]
    fn choose_one_or_both_compares_both_effects_together() {
        assert_eq!(picks(2, 1, 2), vec![vec![0], vec![1], vec![0, 1]]);
    }

    #[test]
    fn zero_modes_and_impossible_counts_are_handled() {
        assert_eq!(picks(2, 0, 1), vec![vec![], vec![0], vec![1]]);
        assert_eq!(picks(0, 0, 0), vec![Vec::<u8>::new()]);
        assert!(picks(1, 2, 2).is_empty());
        assert!(picks(3, 2, 1).is_empty());
    }

    #[test]
    fn large_combinations_stop_at_the_search_limit() {
        let plans = picks(256, 128, 128);
        assert_eq!(plans.len(), 24);
        assert!(
            plans
                .iter()
                .all(|p| p.len() == 128 && p.windows(2).all(|w| w[0] < w[1]))
        );
    }
}
