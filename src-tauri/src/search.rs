use nucleo_matcher::{
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
    Config, Matcher, Utf32Str,
};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchOptions {
    pub candidate_limit: usize,
    pub result_limit: usize,
    pub rrf_k: u32,
}

impl Default for SearchOptions {
    fn default() -> Self {
        // ponytail: bounded recall; raise these limits if boards need more than 1,000 search hits.
        Self {
            candidate_limit: 1000,
            result_limit: 1000,
            rrf_k: 60,
        }
    }
}

impl SearchOptions {
    pub fn valid(self) -> bool {
        (1..=10_000).contains(&self.candidate_limit)
            && (1..=10_000).contains(&self.result_limit)
            && (1..=10_000).contains(&self.rrf_k)
    }
}

pub fn title_priority(title: &str, lowercase_query: &str) -> u8 {
    let title = title.to_lowercase();
    if title == lowercase_query {
        2
    } else if title.starts_with(lowercase_query) {
        1
    } else {
        0
    }
}

pub struct FuzzySearch {
    atom: Atom,
    matcher: Matcher,
    buffer: Vec<char>,
    query: String,
    limit: usize,
    // The smallest score (and largest ID on ties) stays at the root.
    best: BinaryHeap<Reverse<(u16, Reverse<i64>, u8)>>,
}

impl FuzzySearch {
    pub fn new(query: &str, limit: usize) -> Self {
        Self {
            atom: Atom::new(
                query,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
                false,
            ),
            matcher: Matcher::new(Config::DEFAULT),
            buffer: Vec::new(),
            query: query.to_lowercase(),
            limit,
            best: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, id: i64, title: &str) {
        if let Some(score) = self
            .atom
            .score(Utf32Str::new(title, &mut self.buffer), &mut self.matcher)
        {
            let candidate = Reverse((score, Reverse(id), title_priority(title, &self.query)));
            if self.best.len() < self.limit {
                self.best.push(candidate);
            } else if self.best.peek().is_some_and(|worst| candidate < *worst) {
                self.best.pop();
                self.best.push(candidate);
            }
        }
    }

    pub fn finish(self) -> Vec<(i64, u8)> {
        self.best
            .into_sorted_vec()
            .into_iter()
            .map(|Reverse((_, Reverse(id), priority))| (id, priority))
            .collect()
    }
}

pub fn fuse(fts: &[(i64, u8)], fuzzy: &[(i64, u8)], options: SearchOptions) -> Vec<i64> {
    let mut scores = HashMap::<i64, (u8, f64)>::new();
    for ranking in [fts, fuzzy] {
        for (index, &(id, priority)) in ranking.iter().enumerate() {
            let entry = scores.entry(id).or_default();
            entry.0 = entry.0.max(priority);
            entry.1 += 1.0 / (f64::from(options.rrf_k) + (index + 1) as f64);
        }
    }
    let mut ranked: Vec<_> = scores.into_iter().collect();
    ranked.sort_unstable_by(|(a_id, a), (b_id, b)| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a_id.cmp(b_id))
    });
    ranked
        .into_iter()
        .take(options.result_limit)
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_keeps_single_engine_hits_and_prioritizes_exact_then_prefix() {
        let options = SearchOptions::default();
        assert_eq!(
            fuse(
                &[(4, 0), (2, 0)],
                &[(3, 2), (5, 1), (2, 0), (1, 0)],
                options
            ),
            vec![3, 5, 2, 4, 1]
        );
        assert_eq!(
            fuse(&[(2, 0), (1, 0)], &[(1, 0), (2, 0)], options),
            vec![1, 2]
        );
    }

    #[test]
    fn fuzzy_handles_unicode_literal_queries_and_top_k() {
        for (query, title) in [
            ("wRn", "Write Release Notes"),
            ("臺報", "臺灣工作報告"),
            ("臺R", "臺灣 Release"),
            ("🚀報", "🚀 工作報告"),
            ("^a", "^alpha"),
        ] {
            let mut search = FuzzySearch::new(query, 2);
            search.push(1, title);
            assert_eq!(search.finish().len(), 1, "{query}");
        }
        let mut search = FuzzySearch::new("alpha", 2);
        for (id, title) in [
            (4, "a long phrase"),
            (3, "Alpha beta"),
            (2, "Alpha"),
            (1, "Alpha"),
        ] {
            search.push(id, title);
        }
        assert_eq!(search.finish(), vec![(1, 2), (2, 2)]);
    }

    #[test]
    fn options_are_bounded_and_partial_configuration_preserves_defaults() {
        let options: SearchOptions = serde_json::from_str(r#"{"result_limit":1}"#).unwrap();
        assert_eq!(options.candidate_limit, 1000);
        assert_eq!(options.rrf_k, 60);
        assert!(options.valid());
        assert!(!SearchOptions {
            candidate_limit: 0,
            ..options
        }
        .valid());
        assert!(!SearchOptions {
            result_limit: 10_001,
            ..options
        }
        .valid());
        assert!(!SearchOptions {
            rrf_k: 0,
            ..options
        }
        .valid());
        assert_eq!(fuse(&[(1, 0), (2, 0)], &[], options), vec![1]);
    }
}
