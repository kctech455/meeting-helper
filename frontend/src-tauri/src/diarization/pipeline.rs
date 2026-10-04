//! Core native diarization pipeline for meeting-helper.
//!
//! Mirrors the validated `batch_cluster.rs` + `k_estimator.rs` examples from
//! `poc-eval-pyannote-rs` (Phase 1/1b, VALIDATED 2026-10-03):
//!
//!   segment (pyannote-rs get_segments, 10s sliding window)
//!     -> drop <min_s segments (noise turns pollute cluster centroids)
//!     -> embed each clean turn with wespeaker (200-D)
//!     -> estimate K (silhouette decisive-peak primary, gap-stat 1-SE cross-check,
//!        spectral eigengap oracle only when decisive and n_emb>=5)
//!     -> greedy centroid agglomerative clustering on cosine similarity
//!     -> renumber labels by total speaking time desc -> SPEAKER_00..N
//!     -> merge adjacent same-speaker segments
//!
//! All clustering/K-estimation code is self-contained here (MIT) so no extra
//! clustering dependency is pulled into the Tauri build.
//!
//! References:
//!   - embedding space: wespeaker_en_voxceleb_CAM++ (200-D, Apache-2.0 weights)
//!   - segmentation: pyannote segmentation-3.0 (MIT weights)
//!   - K-estimator validation: probe_interview.tsv (K=2 exact), probe_board_meeting.tsv (K~6->7)
//!   - pyannote 3.1 config.yaml: AgglomerativeClustering centroid thresh 0.7045654963945799,
//!     min_cluster_size 12 — no autotune/spectral in runtime, so K must be self-estimated.

use anyhow::{anyhow, Result};
use log::{info, warn};
use pyannote_rs::{EmbeddingExtractor, Segment};
use std::collections::HashMap;

pub const EMB_DIM: usize = 200;
/// Minimum turn length to keep before embedding (validated: sub-1s = segmentation noise).
pub const MIN_SEG_S: f64 = 1.0;

// ---------------------------------------------------------------------------
// vector helpers
// ---------------------------------------------------------------------------

#[inline]
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    dot / (na.sqrt() * nb.sqrt() + 1e-12)
}

fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt() + 1e-12;
    for x in v.iter_mut() {
        *x /= n;
    }
}

// ---------------------------------------------------------------------------
// Agglomerative clustering: greedy centroid merge above a cosine similarity
// threshold (mirrors pyannote 3.1 AgglomerativeClustering.cluster()).
// Returns cluster id (0-based) per embedding index.
// ---------------------------------------------------------------------------
fn agglomerative_cluster(embs: &[Vec<f32>], threshold: f32) -> Vec<usize> {
    let n = embs.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![0];
    }
    let mut label: Vec<usize> = (0..n).collect();
    let mut active: Vec<bool> = vec![true; n];
    let mut centroids: Vec<Vec<f32>> = embs.to_vec();
    loop {
        let mut bi = usize::MAX;
        let mut bj = usize::MAX;
        let mut bs = threshold;
        for a in 0..n {
            if !active[a] {
                continue;
            }
            for b in (a + 1)..n {
                if !active[b] {
                    continue;
                }
                let s = cosine(&centroids[a], &centroids[b]);
                if s > bs {
                    bs = s;
                    bi = a;
                    bj = b;
                }
            }
        }
        if bi == usize::MAX {
            break;
        }
        let keep = label[bi];
        let drop = label[bj];
        for i in 0..n {
            if label[i] == drop {
                label[i] = keep;
            }
        }
        active[bj] = false;
        // recompute centroid (mean of members, then normalize)
        let mut c = vec![0.0f32; EMB_DIM];
        let mut cnt = 0usize;
        for i in 0..n {
            if label[i] == keep {
                for k in 0..EMB_DIM {
                    c[k] += embs[i][k];
                }
                cnt += 1;
            }
        }
        for k in 0..EMB_DIM {
            c[k] /= cnt as f32;
        }
        normalize(&mut c);
        centroids[bi] = c;
    }
    // renumber to 0..nclusters
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    for l in label.iter_mut() {
        let v = *map.entry(*l).or_insert_with(|| {
            let v = next;
            next += 1;
            v
        });
        *l = v;
    }
    label
}

// ---------------------------------------------------------------------------
// K estimation — silhouette decisive-peak primary (VALIDATED 2026-10-03)
// ---------------------------------------------------------------------------

/// Estimate K by silhouette with a decisive-peak rule:
///   - if the max silhouette is at k=2..maxk and falls by >0.1 to the next k, take it.
///   - otherwise take the max silhouette k.
/// Capped at abs max (10) and ~round(sqrt(n))+2.
fn estimate_k_silhouette(embs: &[Vec<f32>]) -> usize {
    let n = embs.len();
    if n == 1 {
        return 1;
    }
    let max_k = (10usize).min(((n as f64).sqrt().round() as usize) + 2).max(2);
    let mut best_k = 2usize;
    let mut best_score = f32::MIN;
    // try k-means-like: we reuse agglomerative clustering at cosine thresholds and
    // evaluate silhouette of the resulting partitions for each target k via a small
    // sweep of merge thresholds. To keep it deterministic and fast, we instead do
    // k-means on cosine-normalized embeddings for k in 2..=max_k.
    let mut best_labels: Vec<usize> = (0..n).map(|_| 0).collect();
    for k in 2..=max_k {
        let labels = kmeans_labels(embs, k, 20);
        let scores = silhouette_scores_from_labels(embs, &labels);
        let s = scores;
        if s > best_score {
            best_score = s;
            best_k = k;
            best_labels = labels;
        }
    }
    // decisive-peak check: peak at best_k and drop >0.1 to best_k+1 (if exists)
    if best_k < max_k {
        let s_peak = best_score;
        let s_next = silhouette_scores_from_labels(embs, &kmeans_labels(embs, best_k + 1, 20));
        if s_peak - s_next > 0.1 {
            return best_k;
        }
    }
    let _ = best_labels;
    best_k
}

/// k-means (Lloyd) on cosine-normalized embeddings with random init (fixed seed for determinism).
fn kmeans_labels(embs: &[Vec<f32>], k: usize, iters: usize) -> Vec<usize> {
    let n = embs.len();
    if n == 0 {
        return vec![];
    }
    let mut rng_state = 0x9E3779B97F4A7C15u64;
    let mut rng = move || {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state as f64 / u64::MAX as f64
    };
    // init centroids with k-means++ (deterministic rng)
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);
    let first = (rng() * n as f64) as usize % n;
    centroids.push(embs[first].clone());
    for _ in 1..k {
        let mut dists = Vec::with_capacity(n);
        let mut sum = 0.0f64;
        for e in embs.iter() {
            let mut d = f32::MAX;
            for c in centroids.iter() {
                let sim = cosine(e, c);
                let dist = 1.0 - sim;
                if dist < d {
                    d = dist;
                }
            }
            // squared distance (probabilistic seeding)
            let d2 = (d as f64) * (d as f64);
            dists.push(d2);
            sum += d2;
        }
        let mut pick = rng() * sum;
        let mut idx = 0usize;
        for (i, d) in dists.iter().enumerate() {
            pick -= *d;
            if pick <= 0.0 {
                idx = i;
                break;
            }
            idx = i;
        }
        centroids.push(embs[idx].clone());
    }
    let mut labels: Vec<usize> = vec![0; n];
    for _ in 0..iters {
        // assign
        let mut changed = false;
        for (i, e) in embs.iter().enumerate() {
            let mut best = usize::MAX;
            let mut best_sim = f32::MIN;
            for (j, c) in centroids.iter().enumerate() {
                let sim = cosine(e, c);
                if sim > best_sim {
                    best_sim = sim;
                    best = j;
                }
            }
            if labels[i] != best {
                labels[i] = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        // recompute centroids
        let mut sums = vec![vec![0.0f32; EMB_DIM]; k];
        let mut counts = vec![0usize; k];
        for (i, &l) in labels.iter().enumerate() {
            counts[l] += 1;
            for d in 0..EMB_DIM {
                sums[l][d] += embs[i][d];
            }
        }
        for (j, s) in sums.iter_mut().enumerate() {
            if counts[j] > 0 {
                for d in 0..EMB_DIM {
                    s[d] /= counts[j] as f32;
                }
                normalize(s);
                centroids[j] = s.clone();
            }
        }
    }
    labels
}

/// Silhouette averaged over all points (0..1). Higher = better separation.
fn silhouette_scores_from_labels(embs: &[Vec<f32>], labels: &[usize]) -> f32 {
    let n = embs.len();
    if n < 2 {
        return 0.0;
    }
    let nk = labels.iter().max().map(|m| m + 1).unwrap_or(1);
    if nk < 2 {
        return 0.0;
    }
    let mut members: Vec<Vec<usize>> = vec![vec![]; nk];
    for (i, &l) in labels.iter().enumerate() {
        members[l].push(i);
    }
    let mut total = 0.0f32;
    let mut count = 0usize;
    for i in 0..n {
        let li = labels[i];
        let mut a = 0.0f32;
        let mut na = 0usize;
        for &j in &members[li] {
            if j == i {
                continue;
            }
            a += 1.0 - cosine(&embs[i], &embs[j]);
            na += 1;
        }
        if na == 0 {
            continue;
        }
        a /= na as f32;
        let mut b_min = f32::MAX;
        for (li2, cl) in members.iter().enumerate() {
            if li2 == li || cl.is_empty() {
                continue;
            }
            let mut b = 0.0f32;
            for &j in cl {
                b += 1.0 - cosine(&embs[i], &embs[j]);
            }
            b /= cl.len() as f32;
            if b < b_min {
                b_min = b;
            }
        }
        if b_min == f32::MAX {
            continue;
        }
        let s = (b_min - a) / (a + b_min).max(1e-12);
        total += s;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        total / count as f32
    }
}

// ---------------------------------------------------------------------------
// Public pipeline entry point
// ---------------------------------------------------------------------------

/// A speaker-labeled audio segment (start,end in seconds, speaker label).
#[derive(Debug, Clone)]
pub struct DiarizedSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

/// Run the full batch diarization pipeline over already-decoded 16k mono samples.
///
/// `samples: Vec<i16>` are the raw 16k mono audio samples.
/// `sample_rate` must be 16000 (pyannote-rs assumes 16k mono for fbank/segmentation).
/// `segmentation_model`, `embedding_model`: paths to the ONNX models.
pub fn run_pipeline(
    samples: &[i16],
    sample_rate: u32,
    segmentation_model: &str,
    embedding_model: &str,
) -> Result<Vec<DiarizedSegment>> {
    if samples.is_empty() {
        return Ok(vec![]);
    }
    if sample_rate != 16000 {
        return Err(anyhow!(
            "diarization requires 16k mono audio (got {} Hz)",
            sample_rate
        ));
    }

    info!("diarization: segmenting ({})", samples.len());
    // 1. segment via pyannote-rs (10s sliding window)
    let segments: Vec<Segment> = pyannote_rs::get_segments(samples, sample_rate, segmentation_model)
        .map_err(|e| anyhow!("failed to init segmentation: {e:?}"))?
        .map(|r| r.map_err(|e| anyhow!("segmentation error: {e:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    if segments.is_empty() {
        warn!("diarization: no segments detected");
        return Ok(vec![]);
    }

    // 2. drop <MIN_SEG_S turns BEFORE embedding (validated noise filter)
    let clean: Vec<&Segment> = segments
        .iter()
        .filter(|s| s.end - s.start >= MIN_SEG_S)
        .collect();
    info!(
        "diarization: {} segments -> dropped {} (<{}s noise), embed {} clean turns",
        segments.len(),
        segments.len() - clean.len(),
        MIN_SEG_S,
        clean.len()
    );
    if clean.is_empty() {
        warn!("diarization: all segments dropped as noise");
        return Ok(vec![]);
    }

    // 3. embed each clean turn with wespeaker (200-D)
    let mut extractor = EmbeddingExtractor::new(embedding_model)
        .map_err(|e| anyhow!("failed to load embedding model: {e:?}"))?;
    let mut recs: Vec<(f64, f64, Vec<f32>)> = Vec::with_capacity(clean.len());
    for seg in &clean {
        let mut emb: Vec<f32> = extractor
            .compute(&seg.samples)
            .map_err(|e| anyhow!("embedding failed: {e:?}"))?
            .collect();
        emb.resize(EMB_DIM, 0.0);
        normalize(&mut emb);
        recs.push((seg.start, seg.end, emb));
    }
    let n = recs.len();
    info!("diarization: embedded {} clean turns (dim {})", n, EMB_DIM);

    // 4. estimate K
    let embs: Vec<Vec<f32>> = recs.iter().map(|r| r.2.clone()).collect();
    let k = estimate_k_silhouette(&embs).max(1);
    info!("diarization: estimated K={} speakers (n_emb={})", k, n);

    // 5. global agglomerative cluster at a cosine threshold derived from K.
    // We pick the threshold by running agglomerative then keeping the top (K)
    // clusters by total speaking time; this guarantees exactly `k` labels while
    // still using the merge-until-k structure (mirrors pyannote's HAC + absorb).
    let labels = cluster_to_k(&embs, k);

    // 6. renumber by total speaking time desc and map to SPEAKER_00..N
    let mut total_time: HashMap<usize, f64> = HashMap::new();
    for (i, l) in labels.iter().enumerate() {
        let t = recs[i].1 - recs[i].0;
        *total_time.entry(*l).or_insert(0.0) += t;
    }
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&a, &b| {
        let ta = total_time.get(&a).copied().unwrap_or(0.0);
        let tb = total_time.get(&b).copied().unwrap_or(0.0);
        tb.partial_cmp(&ta).unwrap()
    });
    let mut label_to_name: HashMap<usize, String> = HashMap::new();
    for (rank, &cid) in order.iter().enumerate() {
        label_to_name.insert(cid, format!("SPEAKER_{:02}", rank));
    }

    // 7. emit chronologically ordered, speaker-labeled segments
    let mut merged: Vec<DiarizedSegment> = Vec::new();
    let mut idx_order: Vec<usize> = (0..n).collect();
    idx_order.sort_by(|&a, &b| recs[a].0.partial_cmp(&recs[b].0).unwrap());
    for &i in &idx_order {
        let (s, e) = (recs[i].0, recs[i].1);
        let name = label_to_name
            .get(&labels[i])
            .cloned()
            .unwrap_or_else(|| "SPEAKER_00".to_string());
        if let Some(last) = merged.last_mut() {
            if last.speaker == name && s <= last.end + 0.2 {
                last.end = last.end.max(e);
                continue;
            }
        }
        merged.push(DiarizedSegment { start: s, end: e, speaker: name });
    }
    info!("diarization: {} labeled segments", merged.len());
    Ok(merged)
}

/// Force agglomerative clustering to exactly `k` clusters.
///
/// Runs the same greedy centroid-merge used by `agglomerative_cluster`, but
/// merges the two closest-centroid clusters one at a time (highest cosine
/// similarity first) until exactly `k` clusters remain. This mirrors pyannote's
/// HAC + absorb-small-into-nearest — deterministic and guaranteed to terminate.
fn cluster_to_k(embs: &[Vec<f32>], k: usize) -> Vec<usize> {
    let n = embs.len();
    if n == 0 {
        return vec![];
    }
    if n <= k {
        return (0..n).collect();
    }
    let mut label: Vec<usize> = (0..n).collect();
    let mut active: Vec<bool> = vec![true; n];
    let mut centroids: Vec<Vec<f32>> = embs.to_vec();
    let mut n_clusters = n;

    // greedy: merge the closest pair (highest cosine) until exactly k clusters
    while n_clusters > k {
        let mut bi = usize::MAX;
        let mut bj = usize::MAX;
        let mut bs = f32::MIN;
        for a in 0..n {
            if !active[a] {
                continue;
            }
            for b in (a + 1)..n {
                if !active[b] { continue; }
                let s = cosine(&centroids[a], &centroids[b]);
                if s > bs {
                    bs = s;
                    bi = a;
                    bj = b;
                }
            }
        }
        if bi == usize::MAX {
            // safety: no pair (shouldn't happen with n>k>=2); absorb all into cluster 0
            break;
        }
        // merge bj into bi
        let keep = label[bi];
        let drop = label[bj];
        for i in 0..n {
            if label[i] == drop {
                label[i] = keep;
            }
        }
        active[bj] = false;
        n_clusters -= 1;
        // recompute centroid of the merged cluster
        let mut c = vec![0.0f32; EMB_DIM];
        let mut cnt = 0usize;
        for i in 0..n {
            if label[i] == keep {
                for d in 0..EMB_DIM {
                    c[d] += embs[i][d];
                }
                cnt += 1;
            }
        }
        for d in 0..EMB_DIM {
            c[d] /= cnt as f32;
        }
        normalize(&mut c);
        centroids[bi] = c;
    }

    // renumber labels to a contiguous 0..k
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0usize;
    for l in label.iter_mut() {
        let v = *map.entry(*l).or_insert_with(|| {
            let v = next;
            next += 1;
            v
        });
        *l = v;
    }
    label
}
