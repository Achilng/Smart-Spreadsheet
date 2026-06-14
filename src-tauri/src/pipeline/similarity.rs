use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimilarityMode {
    Artists,
    PositivePrompt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedGroup {
    pub name: String,
    pub row_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestProgress {
    pub phase: &'static str,
    pub current: usize,
    pub total: usize,
}

pub fn token_jaccard(a: &str, b: &str) -> f64 {
    let set_a: HashSet<&str> = a.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    let set_b: HashSet<&str> = b.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let union_size = set_a.union(&set_b).count();
    if union_size == 0 {
        return 0.0;
    }
    set_a.intersection(&set_b).count() as f64 / union_size as f64
}

pub fn suggest_groups(
    rows: Vec<(i64, String)>,
    mode: SimilarityMode,
    threshold: f64,
    progress: impl Fn(SuggestProgress),
) -> Vec<SuggestedGroup> {
    if rows.is_empty() {
        return Vec::new();
    }

    // Phase 1: Group by exact key to reduce comparison count
    progress(SuggestProgress { phase: "grouping", current: 0, total: rows.len() });
    let mut buckets: HashMap<String, Vec<i64>> = HashMap::new();
    for (id, key) in &rows {
        let normalized = key.trim().to_lowercase();
        buckets.entry(normalized).or_default().push(*id);
    }

    let bucket_entries: Vec<(String, Vec<i64>)> = buckets.into_iter().collect();
    let n = bucket_entries.len();

    if n == 0 {
        return Vec::new();
    }

    // Phase 2: Pairwise similarity between distinct bucket keys
    let total_pairs = if n > 1 { n * (n - 1) / 2 } else { 0 };
    progress(SuggestProgress { phase: "comparing", current: 0, total: total_pairs });

    let mut uf = UnionFind::new(n);
    let mut pair_count = 0usize;

    for i in 0..n {
        for j in (i + 1)..n {
            let sim = match mode {
                SimilarityMode::Artists => {
                    strsim::jaro_winkler(&bucket_entries[i].0, &bucket_entries[j].0)
                }
                SimilarityMode::PositivePrompt => {
                    token_jaccard(&bucket_entries[i].0, &bucket_entries[j].0)
                }
            };
            if sim >= threshold {
                uf.union(i, j);
            }
            pair_count += 1;
            if pair_count.is_multiple_of(5000) {
                progress(SuggestProgress { phase: "comparing", current: pair_count, total: total_pairs });
            }
        }
    }
    progress(SuggestProgress { phase: "comparing", current: total_pairs, total: total_pairs });

    // Phase 3: Collect connected components
    let mut components: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        components.entry(uf.find(i)).or_default().push(i);
    }

    let mut groups: Vec<SuggestedGroup> = Vec::new();
    for indices in components.values() {
        if indices.len() < 2 && indices.iter().all(|&i| bucket_entries[i].1.len() < 2) {
            continue;
        }
        let mut row_ids: Vec<i64> = Vec::new();
        for &idx in indices {
            row_ids.extend_from_slice(&bucket_entries[idx].1);
        }
        row_ids.sort_unstable();
        let name = bucket_entries[indices[0]].0.clone();
        let display_name = if name.len() > 40 {
            format!("{}…", &name[..name.floor_char_boundary(37)])
        } else {
            name
        };
        groups.push(SuggestedGroup {
            name: display_name,
            row_ids,
        });
    }
    groups.sort_by(|a, b| b.row_ids.len().cmp(&a.row_ids.len()));
    groups
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_jaccard_identical() {
        assert!((token_jaccard("a, b, c", "a, b, c") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_jaccard_disjoint() {
        assert!((token_jaccard("a, b", "c, d") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_jaccard_partial_overlap() {
        let j = token_jaccard("a, b, c", "b, c, d");
        assert!((j - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn token_jaccard_empty_strings() {
        assert!((token_jaccard("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaro_winkler_similar_artists() {
        let sim = strsim::jaro_winkler("artist:maidcode1023", "artist:maidcode1024");
        assert!(sim > 0.9);
    }

    #[test]
    fn suggest_groups_clusters_similar_artists() {
        let rows = vec![
            (1, "artist:maidcode1023".to_string()),
            (2, "artist:maidcode1023".to_string()),
            (3, "artist:zephyrblaze_x".to_string()),
            (4, "artist:zephyrblaze_x".to_string()),
            (5, "artist:completely_different_person".to_string()),
        ];
        let groups = suggest_groups(rows, SimilarityMode::Artists, 0.85, |_| {});
        assert!(groups.len() >= 2);
        let maid_group = groups.iter().find(|g| g.row_ids.contains(&1)).unwrap();
        assert!(maid_group.row_ids.contains(&2));
        let zephyr_group = groups.iter().find(|g| g.row_ids.contains(&3)).unwrap();
        assert!(zephyr_group.row_ids.contains(&4));
    }

    #[test]
    fn suggest_groups_clusters_similar_prompts() {
        let rows = vec![
            (1, "best quality, 1girl, blue hair, smile".to_string()),
            (2, "best quality, 1girl, blue hair, smile, outdoors".to_string()),
            (3, "worst quality, 1boy, red eyes".to_string()),
        ];
        let groups = suggest_groups(rows, SimilarityMode::PositivePrompt, 0.6, |_| {});
        assert!(!groups.is_empty());
        let girl_group = groups.iter().find(|g| g.row_ids.contains(&1)).unwrap();
        assert!(girl_group.row_ids.contains(&2));
        assert!(!girl_group.row_ids.contains(&3));
    }

    #[test]
    fn suggest_groups_returns_empty_for_singletons() {
        let rows = vec![
            (1, "alpha_beta_gamma".to_string()),
            (2, "xylophone_zebra".to_string()),
            (3, "completely_different".to_string()),
        ];
        let groups = suggest_groups(rows, SimilarityMode::Artists, 0.85, |_| {});
        assert!(groups.is_empty());
    }

    #[test]
    fn suggest_groups_exact_duplicates_form_group() {
        let rows = vec![
            (1, "same thing".to_string()),
            (2, "same thing".to_string()),
            (3, "same thing".to_string()),
        ];
        let groups = suggest_groups(rows, SimilarityMode::Artists, 0.85, |_| {});
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].row_ids.len(), 3);
    }

    #[test]
    fn union_find_basic() {
        let mut uf = UnionFind::new(5);
        uf.union(0, 1);
        uf.union(2, 3);
        uf.union(1, 3);
        assert_eq!(uf.find(0), uf.find(3));
        assert_ne!(uf.find(0), uf.find(4));
    }
}
