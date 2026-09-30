//! Fuzzy subsequence matching, fzf-style.

/// Score for each matched character.
const MATCH: i32 = 16;
/// Extra for a match right after the previous one.
const CONSECUTIVE: i32 = 8;
/// Extra for a match at the start of a word (after `/`, `-`, `_`, `.` or a space).
const BOUNDARY: i32 = 8;
/// Extra for an upper-case letter following a lower-case one (camelCase).
const CAMEL: i32 = 6;
/// Cost of each character skipped between two matches.
const GAP: i32 = 1;

/// A successful match: higher scores are better; `positions` are the matched char indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub positions: Vec<usize>,
}

/// Match `query` against `text` as a subsequence, preferring word starts and runs.
/// Case-insensitive unless the query contains an upper-case letter (smart case).
/// An empty query matches everything with score 0.
pub fn fuzzy_match(query: &str, text: &str) -> Option<Match> {
    let query: Vec<char> = query.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (m, n) = (query.len(), text.len());
    if m == 0 {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    if m > n {
        return None;
    }
    let case_sensitive = query.iter().any(|c| c.is_uppercase());
    let fold = |c: char| {
        if case_sensitive {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let bonus: Vec<i32> = (0..n)
        .map(|j| {
            let prev = j.checked_sub(1).map(|p| text[p]);
            match prev {
                None => BOUNDARY,
                Some('/' | '-' | '_' | '.' | ' ') => BOUNDARY,
                Some(p) if p.is_lowercase() && text[j].is_uppercase() => CAMEL,
                _ => 0,
            }
        })
        .collect();

    // score[i][j]: best score with query[i] matched at text[j]; back[i][j]: where query[i - 1] went.
    let mut score = vec![vec![None::<i32>; n]; m];
    let mut back = vec![vec![0usize; n]; m];
    for i in 0..m {
        // Best (score, index) of query[i - 1] at k <= j - 2, minus the gap cost up to j.
        let mut gap: Option<(i32, usize)> = None;
        for j in i..n {
            if i > 0 && j >= 2 {
                let decayed = gap.map(|(s, k)| (s - GAP, k));
                let fresh = score[i - 1][j - 2].map(|s| (s - GAP, j - 2));
                gap = match (decayed, fresh) {
                    (Some(a), Some(b)) => Some(if b.0 >= a.0 { b } else { a }),
                    (a, b) => a.or(b),
                };
            }
            if fold(text[j]) != fold(query[i]) {
                continue;
            }
            let here = MATCH + bonus[j];
            if i == 0 {
                score[i][j] = Some(here);
                continue;
            }
            let run = score[i - 1][j - 1].map(|s| (s + CONSECUTIVE, j - 1));
            let best = match (run, gap) {
                (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
                (a, b) => a.or(b),
            };
            if let Some((s, k)) = best {
                score[i][j] = Some(s + here);
                back[i][j] = k;
            }
        }
    }

    // Earliest end wins ties, which also favours shorter spans.
    let (best, mut j) = (0..n)
        .filter_map(|j| score[m - 1][j].map(|s| (s, j)))
        .fold(None, |acc: Option<(i32, usize)>, (s, j)| match acc {
            Some((a, _)) if a >= s => acc,
            _ => Some((s, j)),
        })?;
    let mut positions = vec![0; m];
    for i in (0..m).rev() {
        positions[i] = j;
        j = back[i][j];
    }
    Some(Match {
        score: best,
        positions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(q: &str, t: &str) -> Option<Vec<usize>> {
        fuzzy_match(q, t).map(|m| m.positions)
    }

    fn score(q: &str, t: &str) -> i32 {
        fuzzy_match(q, t).unwrap().score
    }

    #[test]
    fn subsequence_only() {
        assert_eq!(positions("fb", "foo-bar"), Some(vec![0, 4]));
        assert_eq!(positions("bf", "foo-bar"), None);
        assert_eq!(positions("xyz", "xy"), None);
        assert_eq!(positions("", "anything"), Some(vec![]));
    }

    #[test]
    fn smart_case() {
        assert!(fuzzy_match("feat", "Feature").is_some());
        assert!(fuzzy_match("Feat", "feature").is_none());
        assert!(fuzzy_match("Feat", "Feature").is_some());
    }

    #[test]
    fn prefers_word_starts_and_runs() {
        // "login" as a run beats scattered letters.
        assert_eq!(positions("log", "l-o-g-login"), Some(vec![6, 7, 8]));
        // The "b" after "/" beats the earlier inner "b".
        assert_eq!(positions("fb", "fabric/bug"), Some(vec![0, 7]));
        assert!(score("login", "feature/login") > score("login", "fix-lo-gin"));
        assert!(score("fl", "feature/login") > score("fl", "fail"));
    }

    #[test]
    fn multibyte() {
        assert_eq!(positions("ü", "fix/über"), Some(vec![4]));
    }
}
