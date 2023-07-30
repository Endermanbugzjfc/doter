use clap::{Parser, ValueEnum};
use doter::keymap;
use std::{ffi::OsStr, path::PathBuf};
use walkdir::WalkDir;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ConfigFormat {
    File,
    Toml,
    Yaml,
}

#[cfg(target_os = "windows")]
const PACKAGES_PATH: &str = "%APPDATA%/Sublime Text/Packages";
#[cfg(target_os = "macos")]
const PACKAGES_PATH: &str = "~/Library/Application Support/Sublime Text/Packages";
#[cfg(target_os = "linux")]
const PACKAGES_PATH: &str = "~/.config/sublime-text/Packages";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: Option<String>,
    #[arg(long, value_enum, default_value_t=ConfigFormat::File)]
    config_format: ConfigFormat,
    #[arg(short, long, default_value=PathBuf::from(PACKAGES_PATH).into_os_string())]
    packages: PathBuf,
}

fn main() {
    let args = Args::parse();
    let file_ext_keymap = Some("sublime-keymap".as_ref());
    keymaps(&args.packages, file_ext_keymap);
}

#[derive(thiserror::Error, Debug)]
enum InitError {
    #[error("cannot read file")]
    Io(#[from] std::io::Error),
    #[error("cannot parse content into HJSON")]
    Parse(#[from] deser_hjson::Error),
}

fn keymaps(
    path: &PathBuf,
    file_ext_keymap: Option<&OsStr>,
) -> Vec<Result<Vec<doter::keymap::KeymapEntry>, InitError>> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|file| file.ok())
        .filter(|file| file.path().extension() == file_ext_keymap)
        .map(|file| -> Result<Vec<keymap::KeymapEntry>, InitError> {
            let raw = std::fs::read_to_string(file.path())?;
            Ok(deser_hjson::from_str::<Vec<keymap::KeymapEntry>>(&raw)?)
        })
        .collect()
}
