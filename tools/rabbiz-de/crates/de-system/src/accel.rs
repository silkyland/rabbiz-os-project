//! Accelerometer source for auto-rotation.
//!
//! Physical: Linux IIO (`/sys/bus/iio/devices/iio:deviceN/in_accel_{x,y,z}_raw`,
//! `*_scale`, optional `mount_matrix`). Emulator / virtual backend: the mock
//! store (`ctl mock accel x,y,z` or `ctl mock orientation landscape_cw`),
//! which wins over hardware while set. Readings are in the
//! [`de_api::AccelSample`] frame (m/s^2, y toward the top edge at rest).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use de_api::AccelSample;

const IIO_ROOT: &str = "/sys/bus/iio/devices";
const RESCAN: Duration = Duration::from_secs(30);

/// Parse an IIO mount matrix: `"1, 0, 0; 0, 1, 0; 0, 0, 1"` (row-major).
#[must_use]
pub fn parse_mount_matrix(s: &str) -> Option<[[f32; 3]; 3]> {
    let vals: Vec<f32> = s
        .split([',', ';', '\n'])
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    if vals.len() != 9 {
        return None;
    }
    let mut m = [[0.0; 3]; 3];
    for (i, v) in vals.into_iter().enumerate() {
        m[i / 3][i % 3] = v;
    }
    Some(m)
}

fn read_f32(p: &Path) -> Option<f32> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// One IIO accelerometer.
#[derive(Debug, Clone)]
pub struct IioAccel {
    dir: PathBuf,
    scale: [f32; 3],
    mount: [[f32; 3]; 3],
}

impl IioAccel {
    /// Find the first IIO device under `root` exposing `in_accel_*_raw`.
    #[must_use]
    pub fn find_in(root: &Path) -> Option<Self> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        dirs.into_iter().find_map(|dir| {
            if !dir.join("in_accel_x_raw").exists() {
                return None;
            }
            let shared = read_f32(&dir.join("in_accel_scale"));
            let axis = |a: &str| read_f32(&dir.join(format!("in_accel_{a}_scale"))).or(shared);
            let scale = [axis("x")?, axis("y")?, axis("z")?];
            let mount = ["mount_matrix", "in_mount_matrix", "in_accel_mount_matrix"]
                .iter()
                .find_map(|n| std::fs::read_to_string(dir.join(n)).ok())
                .and_then(|s| parse_mount_matrix(&s))
                .unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            Some(Self { dir, scale, mount })
        })
    }

    #[must_use]
    pub fn read(&self) -> Option<AccelSample> {
        let raw = |a: &str| read_f32(&self.dir.join(format!("in_accel_{a}_raw")));
        let v = [
            raw("x")? * self.scale[0],
            raw("y")? * self.scale[1],
            raw("z")? * self.scale[2],
        ];
        let m = &self.mount;
        let row = |r: usize| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2];
        Some(AccelSample::new(row(0), row(1), row(2)))
    }
}

/// Mock-first, IIO-second sensor handle. Cheap to poll.
pub struct AccelSource {
    root: PathBuf,
    iio: Option<IioAccel>,
    scanned: Option<Instant>,
}

impl Default for AccelSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AccelSource {
    #[must_use]
    pub fn new() -> Self {
        Self::with_root(PathBuf::from(IIO_ROOT))
    }

    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            root,
            iio: None,
            scanned: None,
        }
    }

    /// True once a real accelerometer was found.
    #[must_use]
    pub const fn has_hardware(&self) -> bool {
        self.iio.is_some()
    }

    /// Latest sample: the injected one when the mock holds one, else IIO.
    pub fn poll(&mut self) -> Option<AccelSample> {
        if let Some((x, y, z)) = crate::mock::snapshot().accel {
            return Some(AccelSample::new(x, y, z));
        }
        if self.iio.is_none() && self.scanned.is_none_or(|t| t.elapsed() >= RESCAN) {
            self.scanned = Some(Instant::now());
            self.iio = IioAccel::find_in(&self.root);
        }
        self.iio.as_ref()?.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_matrix_parses() {
        let m = parse_mount_matrix("0, -1, 0; 1, 0, 0; 0, 0, 1").unwrap();
        assert_eq!(m[0], [0.0, -1.0, 0.0]);
        assert_eq!(m[2][2], 1.0);
        assert!(parse_mount_matrix("1, 0, 0").is_none());
    }

    #[test]
    fn iio_device_scaled_and_rotated() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("iio:device3");
        std::fs::create_dir(&d).unwrap();
        for (f, v) in [
            ("in_accel_x_raw", "0"),
            ("in_accel_y_raw", "-1000"),
            ("in_accel_z_raw", "0"),
            ("in_accel_scale", "0.0098"),
            ("mount_matrix", "0, -1, 0; 1, 0, 0; 0, 0, 1"),
        ] {
            std::fs::write(d.join(f), v).unwrap();
        }
        let s = IioAccel::find_in(t.path()).unwrap().read().unwrap();
        // raw (0,-9.8,0) through the matrix -> (9.8, 0, 0): turned CCW.
        assert!((s.x - 9.8).abs() < 1e-3 && s.y.abs() < 1e-3);
        assert_eq!(
            crate::accel::AccelSource::with_root(t.path().to_path_buf())
                .poll()
                .map(|s| de_api::OrientationDetector::classify(s)),
            Some(Some(de_api::Orientation::LandscapeCcw))
        );
    }

    #[test]
    fn no_device_is_none() {
        let t = tempfile::tempdir().unwrap();
        assert!(IioAccel::find_in(t.path()).is_none());
    }
}
