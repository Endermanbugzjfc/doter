use clap::{CommandFactory, Parser, ValueEnum};
use doter::keymap;
use miette::{miette, Diagnostic, LabeledSpan};
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};
use walkdir::WalkDir;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ConfigFormat {
    File,
    Toml,
    Yaml,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
const SUBLIME_TEXT_CONFIG: &str = "Sublime Text";
#[cfg(target_os = "linux")]
const SUBLIME_TEXT_CONFIG: &str = "sublime-text";

fn pretty_default_packages_path() -> OsString {
    dirs::config_dir()
        .map(|mut path| {
            path.push(SUBLIME_TEXT_CONFIG);
            path.push("Packages");
            path.into_os_string()
        })
        .unwrap_or(OsString::new())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: Option<String>,
    #[arg(long, value_enum, default_value_t=ConfigFormat::File)]
    config_format: ConfigFormat,
    #[arg(short, long, default_value=pretty_default_packages_path())]
    packages: PathBuf,
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

fn main() -> miette::Result<()> {
    let args = Args::parse();

    let file_ext_keymap = Some("sublime-keymap".as_ref());
    let keymaps = keymaps_parse(&args.packages, file_ext_keymap);
    match keymaps
        .into_iter()
        .find_map(|maybe_parse| match maybe_parse.err() {
            Some(InitError::WalkDir(err)) => {
                let /*mut*/ args_raw: Vec<String> = std::env::args().collect();
                // args_raw.get_mut(0).expect("bro triggered forbidden magic").replace_range(.., "...");

                let arg_index = Args::command().get_matches().index_of("packages").expect("this retarded programmer forgot to rename a hardcoded string literal after refactoring his spaghetti");
                let arg_start = args_raw.iter().take(arg_index).map(|a| a.len() + 1).sum::<usize>();
                let args_len = args_raw.get(arg_index).expect("this retarded programmer forgot to read the docs before adding random libraries to his spaghetti").len();
                Some(
                    miette! {
                    labels = vec![LabeledSpan::at(arg_start..args_len+arg_start, "Invalid path!")],
                    url = "https://www.sublimetext.com/docs/side_by_side.html",
                    "{err}"
                    }
                    .with_source_code(args_raw.join(" ")),
                )
            }
            _ => None,
        }) {
        Some(err) => Err(err),
        None => Ok(()),
    }?;
    Ok(())
}

#[derive(thiserror::Error, Debug, Diagnostic)]
enum InitError {
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] deser_hjson::Error),
}

#[derive(Debug)]
enum KeymapsParse {
    Skipped,
    Parsed(Vec<keymap::KeymapEntry>),
}

fn keymaps_parse(
    path: &PathBuf,
    file_ext_keymap: Option<&OsStr>,
) -> Vec<Result<(PathBuf, KeymapsParse), InitError>> {
    WalkDir::new(path)
        .into_iter()
        .map(|file| {
            let file = file?;
            let path = file.path();
            Ok((path.to_path_buf(), {
                if path.extension() == file_ext_keymap {
                    let raw = std::fs::read_to_string(path)?;
                    KeymapsParse::Parsed(deser_hjson::from_str::<Vec<keymap::KeymapEntry>>(&raw)?)
                } else {
                    KeymapsParse::Skipped
                }
            }))
        })
        .collect()
}
