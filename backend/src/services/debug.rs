use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::Instant,
};

use include_dir::{Dir, include_dir};
use log::debug;
use opencv::{
    core::{Mat, ModifyInplace, Vector},
    imgcodecs::{IMREAD_COLOR, imdecode},
    imgproc::{COLOR_BGR2BGRA, cvt_color_def},
};
use rand::distr::SampleString;
use rand_distr::Alphanumeric;
use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{
    DebugState,
    debug::save_minimap_for_training,
    detect::{ArrowsCalibrating, ArrowsState, DefaultDetector, Detector},
    ecs::Resources,
    mat::OwnedMat,
    models::Localization,
    utils::{self, DatasetDir},
};

const SOLVE_RUNE_TIMEOUT_SECS: u64 = 10;

#[derive(Debug)]
pub struct DebugService {
    state: Sender<DebugState>,
    recording_id: Option<String>,
    infering_rune: Option<(ArrowsCalibrating, Instant)>,
}

impl Default for DebugService {
    fn default() -> Self {
        Self {
            state: broadcast::channel(1).0,
            recording_id: None,
            infering_rune: None,
        }
    }
}

impl DebugService {
    pub fn poll(&mut self, resources: &Resources) {
        if let Some(id) = self.recording_id.clone() {
            utils::save_image_to(
                resources.detector().mat(),
                DatasetDir::Root,
                PathBuf::from(id),
            );
        }

        if let Some((calibrating, instant)) = self.infering_rune.as_ref().copied() {
            if instant.elapsed().as_secs() >= SOLVE_RUNE_TIMEOUT_SECS {
                self.infering_rune = None;
                debug!(target: "debug", "infer rune timed out");
                return;
            }

            match resources.detector().detect_rune_arrows(calibrating) {
                Ok(ArrowsState::Complete(arrows)) => {
                    // TODO: Save
                    self.infering_rune = None;
                    debug!(target: "debug", "infer rune result {arrows:?}");
                }
                Ok(ArrowsState::Calibrating(calibrating)) => {
                    self.infering_rune = Some((calibrating, instant));
                }
                Err(err) => {
                    self.infering_rune = None;
                    debug!(target: "debug", "infer rune failed {err}");
                }
            }
        }

        if self.state.is_empty() {
            let _ = self.state.send(DebugState {
                is_recording: self.recording_id.is_some(),
                is_rune_auto_saving: resources.debug.auto_save_rune(),
            });
        }
    }

    pub fn subscribe_state(&self) -> Receiver<DebugState> {
        self.state.subscribe()
    }

    pub fn set_auto_save_rune(&self, resources: &Resources, auto_save: bool) {
        resources.debug.set_auto_save_rune(auto_save);
    }

    pub fn record_images(&mut self, start: bool) {
        self.recording_id = if start {
            Some(Alphanumeric.sample_string(&mut rand::rng(), 8))
        } else {
            None
        };
    }

    pub fn infer_rune(&mut self) {
        self.infering_rune = Some((ArrowsCalibrating::default(), Instant::now()));
    }

    pub fn infer_minimap(&self, resources: &Resources) {
        if let Some(detector) = resources.detector.as_ref()
            && let Some(bbox) = detector.detect_minimap(160).ok()
        {
            save_minimap_for_training(detector.mat(), bbox);
        }
    }

    pub fn test_spin_rune(&self) {
        static SPIN_TEST_DIR: Dir<'static> = include_dir!("$SPIN_TEST_DIR");
        static SPIN_TEST_IMAGES: LazyLock<Vec<Mat>> = LazyLock::new(|| {
            let mut files = SPIN_TEST_DIR.files().collect::<Vec<_>>();
            files.sort_by_key(|file| file.path().to_str().unwrap());
            files
                .into_iter()
                .map(|file| {
                    let vec = Vector::from_slice(file.contents());
                    let mut mat = imdecode(&vec, IMREAD_COLOR).unwrap();
                    unsafe {
                        mat.modify_inplace(|mat, mat_mut| {
                            cvt_color_def(mat, mat_mut, COLOR_BGR2BGRA).unwrap();
                        });
                    }
                    mat
                })
                .collect()
        });

        let localization = Arc::new(Localization::default());
        let mut calibrating = ArrowsCalibrating::default();
        calibrating.enable_spin_test();

        for mat in &*SPIN_TEST_IMAGES {
            match DefaultDetector::new(OwnedMat::from(mat.clone()), localization.clone())
                .detect_rune_arrows(calibrating)
            {
                Ok(ArrowsState::Complete(arrows)) => {
                    debug!(target: "test", "spin test completed {arrows:?}");
                    break;
                }
                Ok(ArrowsState::Calibrating(new_calibrating)) => {
                    calibrating = new_calibrating;
                }
                Err(err) => {
                    debug!(target: "test", "spin test error {err}");
                    break;
                }
            }
        }
    }
}
