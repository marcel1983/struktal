//! Reverse Cuthill–McKee ordering to reduce skyline profile.

use std::collections::VecDeque;

use crate::sparse::Csc;

/// Returns `perm` with `perm[new] = old`. Deterministic: ties broken by
/// (degree, original index). Each connected component starts from a
/// pseudo-peripheral node found by repeated BFS.
pub fn reverse_cuthill_mckee(a: &Csc) -> Vec<usize> {
    let n = a.n;
    let adj: Vec<Vec<usize>> = (0..n).map(|c| a.column(c).map(|(r, _)| r).filter(|&r| r != c).collect()).collect();
    let deg: Vec<usize> = adj.iter().map(Vec::len).collect();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);

    let bfs_levels = |start: usize, mask: &[bool]| -> (usize, usize) {
        // Returns (farthest node with min degree in last level, eccentricity).
        let mut lvl = vec![usize::MAX; n];
        let mut q = VecDeque::from([start]);
        lvl[start] = 0;
        let mut last = start;
        while let Some(u) = q.pop_front() {
            for &v in &adj[u] {
                if !mask[v] && lvl[v] == usize::MAX {
                    lvl[v] = lvl[u] + 1;
                    q.push_back(v);
                    let better = lvl[v] > lvl[last] || (lvl[v] == lvl[last] && (deg[v], v) < (deg[last], last));
                    if better {
                        last = v;
                    }
                }
            }
        }
        (last, lvl[last])
    };

    let mut seeds: Vec<usize> = (0..n).collect();
    seeds.sort_by_key(|&i| (deg[i], i));
    for &seed in &seeds {
        if visited[seed] {
            continue;
        }
        let mut start = seed;
        let mut ecc = bfs_levels(start, &visited).1;
        for _ in 0..8 {
            let (far, e) = bfs_levels(start, &visited);
            let (_, e2) = bfs_levels(far, &visited);
            if e2 > ecc {
                start = far;
                ecc = e2;
            } else {
                let _ = e;
                break;
            }
        }
        let mut q = VecDeque::from([start]);
        visited[start] = true;
        while let Some(u) = q.pop_front() {
            order.push(u);
            let mut nb: Vec<usize> = adj[u].iter().copied().filter(|&v| !visited[v]).collect();
            nb.sort_by_key(|&v| (deg[v], v));
            for v in nb {
                visited[v] = true;
                q.push_back(v);
            }
        }
    }
    order.reverse();
    order
}

pub fn is_permutation(p: &[usize]) -> bool {
    let mut seen = vec![false; p.len()];
    for &i in p {
        if i >= p.len() || seen[i] {
            return false;
        }
        seen[i] = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::TripletBuilder;

    #[test]
    fn rcm_is_permutation_and_reduces_bandwidth_of_scrambled_path() {
        // Path graph 0-5-1-4-2-3 numbered badly.
        let edges = [(0, 5), (5, 1), (1, 4), (4, 2), (2, 3)];
        let mut t = TripletBuilder::new(6).expect("n");
        for i in 0..6 {
            t.add(i, i, 2.0).expect("ok");
        }
        for (a, b) in edges {
            t.add(a, b, -1.0).expect("ok");
            t.add(b, a, -1.0).expect("ok");
        }
        let a = t.to_csc();
        let p = reverse_cuthill_mckee(&a);
        assert!(is_permutation(&p));
        let mut inv = [0; 6];
        for (new, &old) in p.iter().enumerate() {
            inv[old] = new;
        }
        let bw = edges.iter().map(|&(a, b)| inv[a].abs_diff(inv[b])).max().unwrap_or(0);
        assert_eq!(bw, 1);
    }

    #[test]
    fn disconnected_components_covered() {
        let mut t = TripletBuilder::new(4).expect("n");
        for i in 0..4 {
            t.add(i, i, 1.0).expect("ok");
        }
        t.add(0, 3, 1.0).expect("ok");
        t.add(3, 0, 1.0).expect("ok");
        let p = reverse_cuthill_mckee(&t.to_csc());
        assert!(is_permutation(&p));
    }
}
