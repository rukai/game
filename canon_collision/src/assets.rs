use std::fs::File;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use hotwatch::{Hotwatch, Event};

/// Hot reloadable assets
pub struct Assets {
    path: PathBuf,
    models_reload_rx: Receiver<Reload>,
    //audio_reload_rx: Receiver<Reload>,
    models_reload_tx: Sender<Reload>,
    //audio_reload_tx: Arc<Sender<Reload>,
    hotwatch: Hotwatch,
}

impl Assets {
    pub fn new() -> Option<Self> {
        let (models_reload_tx, models_reload_rx) = mpsc::channel();

        let current_dir = std::env::current_dir().unwrap();
        if let Some(path) = Assets::find_assets_in_parent_dirs_core(&current_dir) {
            Some(Assets {
                path,
                models_reload_rx,
                models_reload_tx,
                hotwatch: Hotwatch::new().unwrap(),
            })
        }
        else {
            None
        }
    }

    fn find_assets_in_parent_dirs_core(path: &Path) -> Option<PathBuf> {
        let assets_path = path.join("assets");
        match fs::metadata(&assets_path) {
            Ok(_) => {
                Some(assets_path.to_path_buf())
            }
            Err(_) => {
                Assets::find_assets_in_parent_dirs_core(path.parent()?)
            }
        }
    }

    pub fn models_reloads(&self) -> Vec<Reload> {
        let mut reloads = vec!();

        while let Ok(reload) = self.models_reload_rx.try_recv() {
            reloads.push(reload);
        }

        reloads
    }

    /// On failure to read from disk, logs the error and returns None
    pub fn get_model(&mut self, name: &str) -> Option<Vec<u8>> {
        let path = self.path.join("models").join(format!("{}.glb", name));
        println!("{:?}", path);

        let tx = self.models_reload_tx.clone();
        let reload_name = name.to_string();
        let reload_path = path.clone();
        self.hotwatch.watch(&path, move |event: Event| {
            let path = reload_path.clone();
            let name = reload_name.clone();
            if let Event::Write(_) = event {
                if let Some(data) = Assets::load_file(path) {
                    tx.send(Reload { name, data }).unwrap();
                }
            }
        }).unwrap();

        Assets::load_file(path)
    }

    /// On failure to read from disk, logs the error and returns None
    fn load_file(path: PathBuf) -> Option<Vec<u8>> {
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                error!("Failed to open file: {} because: {}", path.to_str().unwrap(), err);
                return None;
            }
        };

        let mut contents = Vec::<u8>::new();
        if let Err(err) = file.read_to_end(&mut contents) {
            error!("Failed to read file {} because: {}", path.to_str().unwrap(), err);
            return None;
        };
        Some(contents)
    }
}

pub struct Reload {
    pub name: String,
    pub data: Vec<u8>,
}
