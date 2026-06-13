use std::io;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;

use super::{DataDirectory, StorageError};
use crate::db::ContentHashCandidate;
use crate::pipeline::parallel;

const HASH_SIZE: usize = 8;
const DCT_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerceptualHashProgress {
    pub processed: usize,
    pub total: usize,
    pub updated: usize,
    pub unreadable: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerceptualHashBackfillOutcome {
    pub total: usize,
    pub updated: usize,
    pub unreadable: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimilarImageMatch {
    pub row_id: i64,
    pub distance: u32,
}

impl DataDirectory {
    pub fn backfill_perceptual_hashes(
        &self,
        progress: impl Fn(PerceptualHashProgress),
    ) -> Result<PerceptualHashBackfillOutcome, StorageError> {
        let mut database = self.open_database()?;
        let candidates = database.missing_perceptual_hashes()?;
        let total = candidates.len();
        if total == 0 {
            return Ok(PerceptualHashBackfillOutcome::default());
        }

        progress(PerceptualHashProgress {
            total,
            ..PerceptualHashProgress::default()
        });

        let results = parallel::parallel_map(
            candidates,
            parallel::worker_count(total),
            |_, candidate| {
                let hash = resolve_image_path(self, &candidate)
                    .and_then(|path| compute_phash(&path))
                    .ok();
                (candidate.row_id, hash)
            },
            |completed| {
                progress(PerceptualHashProgress {
                    processed: completed,
                    total,
                    updated: 0,
                    unreadable: 0,
                });
            },
        );

        let mut hashes = Vec::with_capacity(total);
        let mut unreadable = 0;
        for (row_id, hash) in results {
            match hash {
                Some(h) => hashes.push((row_id, h)),
                None => unreadable += 1,
            }
        }
        progress(PerceptualHashProgress {
            processed: total,
            total,
            updated: hashes.len(),
            unreadable,
        });

        database.update_perceptual_hashes(&hashes)?;
        Ok(PerceptualHashBackfillOutcome {
            total,
            updated: hashes.len(),
            unreadable,
        })
    }

    pub fn search_similar_images(
        &self,
        query_path: &Path,
        threshold: u32,
    ) -> Result<Vec<SimilarImageMatch>, StorageError> {
        let query_hash = compute_phash(query_path).map_err(StorageError::Io)?;
        let query_value = u64::from_str_radix(&query_hash, 16)
            .map_err(|e| StorageError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;

        let database = self.open_database()?;
        let all_hashes = database.all_perceptual_hashes()?;

        let mut matches: Vec<SimilarImageMatch> = all_hashes
            .into_iter()
            .filter_map(|(row_id, hash_str)| {
                let stored = u64::from_str_radix(&hash_str, 16).ok()?;
                let distance = hamming_distance(query_value, stored);
                if distance <= threshold {
                    Some(SimilarImageMatch { row_id, distance })
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by_key(|m| m.distance);
        Ok(matches)
    }
}

pub(crate) fn compute_phash(path: &Path) -> Result<String, io::Error> {
    let img = image::open(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let gray = img
        .resize_exact(DCT_SIZE as u32, DCT_SIZE as u32, FilterType::Triangle)
        .to_luma8();

    let pixels: Vec<f64> = gray.pixels().map(|p| p.0[0] as f64).collect();

    // 2D DCT-II：先对行做 DCT，再对列做 DCT。
    let mut row_dct = vec![0.0_f64; DCT_SIZE * DCT_SIZE];
    for y in 0..DCT_SIZE {
        for u in 0..DCT_SIZE {
            let mut sum = 0.0;
            for x in 0..DCT_SIZE {
                sum += pixels[y * DCT_SIZE + x]
                    * ((2.0 * x as f64 + 1.0) * u as f64 * std::f64::consts::PI
                        / (2.0 * DCT_SIZE as f64))
                        .cos();
            }
            row_dct[y * DCT_SIZE + u] = sum;
        }
    }

    let mut dct2 = vec![0.0_f64; DCT_SIZE * DCT_SIZE];
    for u in 0..DCT_SIZE {
        for v in 0..DCT_SIZE {
            let mut sum = 0.0;
            for y in 0..DCT_SIZE {
                sum += row_dct[y * DCT_SIZE + u]
                    * ((2.0 * y as f64 + 1.0) * v as f64 * std::f64::consts::PI
                        / (2.0 * DCT_SIZE as f64))
                        .cos();
            }
            dct2[v * DCT_SIZE + u] = sum;
        }
    }

    // 取左上 8×8 低频系数（跳过 [0,0] 的 DC 分量）。
    let mut low_freq = Vec::with_capacity(HASH_SIZE * HASH_SIZE);
    for y in 0..HASH_SIZE {
        for x in 0..HASH_SIZE {
            low_freq.push(dct2[y * DCT_SIZE + x]);
        }
    }

    let mut sorted = low_freq[1..].to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];

    let mut hash: u64 = 0;
    for (i, &value) in low_freq.iter().enumerate() {
        if value > median {
            hash |= 1_u64 << i;
        }
    }

    Ok(format!("{hash:016x}"))
}

fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn resolve_image_path(
    directory: &DataDirectory,
    candidate: &ContentHashCandidate,
) -> Result<PathBuf, io::Error> {
    if let Some(path) = nonempty(candidate.image_path.as_deref()).map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }
    if let Some(relative) = nonempty(candidate.stored_image_path.as_deref()) {
        let path = directory.root().join(relative);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("第 {} 行没有可读图片", candidate.row_id),
    ))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::db::{NewRow, SourceType};

    use super::*;

    #[test]
    fn phash_is_deterministic_and_16_hex_chars() {
        let temporary = TemporaryPhashDirectory::new();
        let img_path = create_test_png(&temporary.root, "test.png", 64, 64, [200, 100, 50]);

        let hash1 = compute_phash(&img_path).unwrap();
        let hash2 = compute_phash(&img_path).unwrap();

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn similar_images_have_low_hamming_distance() {
        let temporary = TemporaryPhashDirectory::new();
        let original = create_test_png(&temporary.root, "original.png", 128, 128, [200, 100, 50]);
        let similar = create_test_png(&temporary.root, "similar.png", 64, 64, [200, 100, 50]);
        let different =
            create_test_png(&temporary.root, "different.png", 128, 128, [10, 220, 180]);

        let hash_orig = compute_phash(&original).unwrap();
        let hash_sim = compute_phash(&similar).unwrap();
        let hash_diff = compute_phash(&different).unwrap();

        let orig_val = u64::from_str_radix(&hash_orig, 16).unwrap();
        let sim_val = u64::from_str_radix(&hash_sim, 16).unwrap();
        let diff_val = u64::from_str_radix(&hash_diff, 16).unwrap();

        let dist_similar = hamming_distance(orig_val, sim_val);
        let dist_different = hamming_distance(orig_val, diff_val);

        assert!(
            dist_similar < dist_different,
            "similar distance {dist_similar} should be less than different distance {dist_different}"
        );
    }

    #[test]
    fn backfill_computes_hashes_for_rows_with_readable_images() {
        let temporary = TemporaryPhashDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let img = create_test_png(&temporary.root, "image.png", 32, 32, [128, 128, 128]);

        let mut database = directory.open_database().unwrap();
        database
            .append_batch(
                SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[
                    NewRow {
                        source_ordinal: 1,
                        identity: "file:readable".into(),
                        image_path: Some(img.to_string_lossy().into_owned()),
                        ..NewRow::default()
                    },
                    NewRow {
                        source_ordinal: 2,
                        identity: "file:missing".into(),
                        image_path: Some(
                            temporary
                                .root
                                .join("missing.png")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                        ..NewRow::default()
                    },
                ],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);

        let events = Mutex::new(Vec::new());
        let outcome = directory
            .backfill_perceptual_hashes(|p| events.lock().unwrap().push(p))
            .unwrap();

        assert_eq!(outcome.total, 2);
        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.unreadable, 1);

        let database = directory.open_database().unwrap();
        let missing = database.missing_perceptual_hashes().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].row_id, 2);

        let all = database.all_perceptual_hashes().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, 1);
        assert_eq!(all[0].1.len(), 16);
    }

    #[test]
    fn search_finds_similar_images_within_threshold() {
        let temporary = TemporaryPhashDirectory::new();
        let directory = DataDirectory::initialize(&temporary.data).unwrap();
        let img = create_test_png(&temporary.root, "stored.png", 64, 64, [200, 100, 50]);
        let query = create_test_png(&temporary.root, "query.png", 32, 32, [200, 100, 50]);

        let mut database = directory.open_database().unwrap();
        let phash = compute_phash(&img).unwrap();
        database
            .append_batch(
                SourceType::Folder,
                &temporary.root.to_string_lossy(),
                &[NewRow {
                    source_ordinal: 1,
                    identity: "file:stored".into(),
                    image_path: Some(img.to_string_lossy().into_owned()),
                    perceptual_hash: Some(phash),
                    ..NewRow::default()
                }],
                |_| Ok(()),
            )
            .unwrap();
        drop(database);

        let matches = directory.search_similar_images(&query, 10).unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches[0].row_id, 1);
    }

    fn create_test_png(dir: &Path, name: &str, w: u32, h: u32, color: [u8; 3]) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let mut img = image::RgbImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgb(color);
        }
        img.save(&path).unwrap();
        path
    }

    struct TemporaryPhashDirectory {
        root: PathBuf,
        data: PathBuf,
    }

    impl TemporaryPhashDirectory {
        fn new() -> Self {
            let parent = Path::new(r"D:\Agent\Agent_temp");
            let parent = if parent.is_dir() {
                parent.to_owned()
            } else {
                std::env::temp_dir()
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be valid")
                .as_nanos();
            let root = parent.join(format!(
                "smart-spreadsheet-phash-{}-{nonce}",
                std::process::id()
            ));
            Self {
                data: root.join("data"),
                root,
            }
        }
    }

    impl Drop for TemporaryPhashDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
