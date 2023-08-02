use clap::{CommandFactory, Parser, ValueEnum};
use doter::keymap;
use miette::{miette, Diagnostic, ErrReport, LabeledSpan, NamedSource};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ConfigFormat {
    File,
    Toml,
    Yaml,
    Hjson,
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
    #[arg(long, default_value_t = 1)]
    max_err: usize,
}

macro_rules! arg_index_of {
    ($field:ident) => {{
        const _: () = {
            fn assert_field(v: Args) {
                drop(v.$field);
            }
        };
        Args::command()
            .get_matches()
            .index_of(stringify!($field))
            .expect("bro triggered forbidden magic")
    }};
}

fn main() -> miette::Result<(), Vec<ErrReport>> {
    let args = Args::parse();

    let file_ext_keymap = Some("sublime-keymap".as_ref());
    let keymaps = keymaps_parse(args.packages.as_path(), file_ext_keymap);
    let reports: Vec<miette::Report> = keymaps
        .into_iter()
        .filter_map(|maybe_parse| -> Option<ErrReport> {
            Some(match maybe_parse.err() {
                Some(InitError::WalkDir(err)) => {
                    let /*mut*/ args_raw: Vec<String> = std::env::args().collect();
                    // args_raw.get_mut(0).expect("bro triggered forbidden magic").replace_range(.., "...");

                    let arg_index = arg_index_of!(packages);
                    let arg_start = args_raw.iter().take(arg_index).map(|arg| arg.len() + 1).sum::<usize>();
                    let args_len = args_raw.get(arg_index).expect("this retarded programmer forgot to read the docs before adding random libraries to his spaghetti").len();
                    miette! {
                    labels = vec![LabeledSpan::new(Some("Invalid path!".to_owned()), arg_start, args_len)],
                    url = "https://www.sublimetext.com/docs/side_by_side.html",
                    "{err}"
                    }
                    .with_source_code(args_raw.join(" "))
                }
                Some(InitError::Parse(path, err, raw)) => {
                    let raw = raw.replace("\r\n", "\n").replace("\r", "");

                    use deser_hjson::Error;
                    let (heading, details): (&str, Option<(&usize, &usize, &str)>) = match &err {
                        Error::Syntax {line, col, ..} => ("Invalid syntax! (we tried our best but is your HJSON made in China?)", Some((line, col, "Error occurred nearby"))),
                        Error::Serde {line, col, message: msg, ..} => ("Invalid data!", Some((line, col, msg))),
                        _ => ("", None),
                    };
                    let labels: Vec<LabeledSpan> = details.into_iter().map(|(line, col, msg)| {
                        let offset = raw.lines().take(line-1).map(|raw_line| raw_line.len() + 1).sum::<usize>() - 1 + col;
                        LabeledSpan::at_offset(offset, msg)
                    }).collect();
                    miette!{
                    labels = labels,
                    "{heading}"
                }.with_source_code(NamedSource::new(path.to_str().unwrap_or("(<File path is not UTF-8>"), raw))}, 
                Some(err) => ErrReport::msg(err),
                None => return None
        })}).take(args.max_err).collect();

    if reports.len() == 0 {
        return Ok(());
    }
    Err(reports)
}

#[derive(thiserror::Error, Debug, Diagnostic)]
enum InitError {
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
    #[error("{1}")]
    Io(PathBuf, std::io::Error),
    #[error("{1}")]
    Parse(PathBuf, deser_hjson::Error, String),
}

impl InitError {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::WalkDir(err) => err.path(),
            Self::Io(path, _) => Some(path.as_path()),
            Self::Parse(path, _, _) => Some(path.as_path()),
        }
    }
}

#[derive(Debug)]
enum KeymapsParse {
    Skipped,
    Parsed(Vec<keymap::KeymapEntry>),
}

fn keymaps_parse(
    path: &Path,
    file_ext_keymap: Option<&OsStr>,
) -> Vec<Result<(PathBuf, KeymapsParse), InitError>> {
    WalkDir::new(path)
        .into_iter()
        .map(|file| {
            let file = file?;
            let path = file.path();
            Ok((path.to_path_buf(), {
                if path.extension() == file_ext_keymap {
                    let raw = std::fs::read_to_string(path)
                        .map_err(|err| InitError::Io(path.to_path_buf(), err))?;
                    let parsed = deser_hjson::from_str::<Vec<keymap::KeymapEntry>>(&raw)
                        .map_err(|err| InitError::Parse(path.to_path_buf(), err, raw))?;
                    KeymapsParse::Parsed(parsed)
                } else {
                    KeymapsParse::Skipped
                }
            }))
        })
        .collect()
}
