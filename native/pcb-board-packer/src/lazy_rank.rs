//! Exact top-k selection when expensive evaluation can only worsen the rank.
use std::cmp::Ordering;

pub(crate) fn top_k<T>(
    mut values: Vec<T>,
    limit: usize,
    compare: impl Fn(&T, bool, &T, bool) -> Ordering,
    mut evaluate: impl FnMut(&mut T),
) -> Vec<T> {
    if limit >= values.len() {
        for value in &mut values { evaluate(value); }
        values.sort_by(|a, b| compare(a, true, b, true));
        return values;
    }
    let mut pending: Vec<_> = values.into_iter().map(|value| (value, false)).collect();
    loop {
        pending.sort_by(|(a, a_exact), (b, b_exact)| compare(a, *a_exact, b, *b_exact));
        let Some(index) = pending.iter().take(limit).position(|(_, exact)| !exact) else { break };
        evaluate(&mut pending[index].0);
        pending[index].1 = true;
    }
    pending.into_iter().take(limit).map(|(value, _)| value).collect()
}

/// Visit candidates in their final rank order, evaluating only bounds that can
/// still improve the incumbent. Evaluation must change only the score (upwards),
/// after the fixed hard-violation/severity prefix used by compare and improves.
pub(crate) fn improve<T, K>(
    values: Vec<T>,
    mut incumbent: K,
    compare: impl Fn(&T, &T) -> Ordering,
    improves: impl Fn(&T, &K) -> bool,
    key: impl Fn(&T) -> K,
    mut evaluate: impl FnMut(&mut T),
) -> Option<T> {
    let mut pending: Vec<_> = values.into_iter().map(|value| (value, false)).collect();
    let mut winner = None;
    while !pending.is_empty() {
        pending.sort_by(|(a, _), (b, _)| compare(a, b));
        let (mut value, exact) = pending.remove(0);
        if !improves(&value, &incumbent) { continue; }
        if !exact {
            evaluate(&mut value);
            pending.push((value, true));
        } else {
            incumbent = key(&value);
            winner = Some(value);
        }
    }
    winner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_selection_matches_eager_with_ties_hard_ranks_and_large_corrections() {
        for seed in 0..100usize {
            let candidates: Vec<_> = (0..32).map(|i| ((i + seed) % 3, (i * 7 + seed) % 19, i, (i * seed) % 53)).collect();
            for limit in [0, 1, 8, 28, 32, 40] {
                let mut eager = candidates.clone();
                for value in &mut eager { value.1 += value.3; }
                eager.sort_by_key(|value| (value.0, value.1, value.2));
                eager.truncate(limit);
                let actual = top_k(candidates.clone(), limit,
                    |a, _, b, _| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)),
                    |value| value.1 += value.3);
                assert_eq!(actual, eager);
            }
        }
    }

    #[test]
    fn clear_winner_requires_only_one_expensive_evaluation() {
        let mut calls = 0;
        let result = top_k(vec![0, 100, 200, 300], 1, |a, _, b, _| a.cmp(b), |value| {
            calls += 1;
            *value += 10;
        });
        assert_eq!(result, vec![10]);
        assert_eq!(calls, 1);
    }

    #[test]
    fn local_improvement_matches_eager_with_severity_epsilon_and_score_ties() {
        use crate::model::Rank;
        use crate::solver::rank_improves;
        let compare = |a: &(Rank, usize, f64), b: &(Rank, usize, f64)| {
            a.0.hard_count.cmp(&b.0.hard_count)
                .then_with(|| a.0.hard_severity.total_cmp(&b.0.hard_severity))
                .then_with(|| a.0.score.total_cmp(&b.0.score))
                .then_with(|| a.1.cmp(&b.1))
        };
        for seed in 0..1000u64 {
            let mut random = seed + 1;
            let mut next = || { random = random.wrapping_mul(6364136223846793005).wrapping_add(1); random >> 32 };
            let incumbent = Rank { hard_count: (next() % 3) as usize,
                hard_severity: (next() % 8) as f64 * 0.0005, score: (next() % 40) as f64 - 10.0 };
            let values: Vec<_> = (0..32).map(|id| (Rank { hard_count: (next() % 3) as usize,
                hard_severity: (next() % 8) as f64 * 0.0005, score: (next() % 40) as f64 - 10.0 },
                id, (next() % 40) as f64 * 0.25)).collect();
            let mut eager = values.clone();
            for value in &mut eager { value.0.score += value.2; }
            eager.sort_by(compare);
            let mut best = incumbent.clone();
            let mut expected = None;
            for value in eager {
                if rank_improves(&value.0, &best) { best = value.0.clone(); expected = Some(value.1); }
            }
            let actual = improve(values, incumbent, compare, |value, best| rank_improves(&value.0, best),
                |value| value.0.clone(), |value| value.0.score += value.2);
            assert_eq!(actual.map(|value| value.1), expected, "seed={seed}");
        }
    }
}
