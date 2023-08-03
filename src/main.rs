use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use doter::keymap;
use miette::{miette, Diagnostic, ErrReport, LabeledSpan, NamedSource, Severity};
use nonempty::{nonempty, NonEmpty};
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

fn main() -> miette::Result<(), NonEmpty<ErrReport>> {
    let args_raw: Vec<String> = std::env::args().collect();
    let args_cmd = Args::command();
    let mut matches = match args_cmd
        .clone()
        .try_get_matches_from_mut(std::env::args_os())
    {
        Ok(matches) => matches,
        Err(err) => return Err(nonempty![miette!("{err}")]),
    };
    let args = Args::from_arg_matches_mut(&mut matches).expect("bro triggered forbidden magic");

    let file_ext_keymap = Some("sublime-keymap".as_ref());
    let keymaps = keymaps_parse(args.packages.as_path(), file_ext_keymap);

    let (reports, io_status): (Vec<Option<miette::Report>>, Vec<Option<(PathBuf, Option<std::io::Error>)>>) = keymaps
        .into_iter()
        .map(|maybe_parse| match maybe_parse {
            Err(InitError::WalkDir(err)) => {
                // args_raw.get_mut(0).expect("bro triggered forbidden magic").replace_range(.., "...");

                let arg_index = arg_index_of!(packages);
                let arg_start = args_raw.iter().take(arg_index).map(|arg| arg.len() + 1).sum::<usize>();
                let args_len = args_raw.get(arg_index).expect("this retarded programmer forgot to read the docs before adding random libraries to his spaghetti").len();
                (Some(miette! {
                labels = vec![LabeledSpan::new(Some("Path is inaccessible".to_owned()), arg_start, args_len)],
                url = "https://www.sublimetext.com/docs/side_by_side.html",
                help = "Look for incorrect spellings or capitalisations. Also, check if the path leads to a DIRECTORY, a file won't work. Don't forget to ensure that all nodes in the path have proper permissions too.",
                "{err}",
                }
                .with_source_code(args_raw.join(" "))), None)
            },
            Err(InitError::Parse(path, err, raw)) => {
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
                (Some(miette!{
                    labels = labels,
                    "{heading}"
                }.with_source_code(NamedSource::new(path.to_str().unwrap_or("(<File path is not UTF-8>"), raw))), Some((path, None)))
            },
            Err(InitError::Io(path, err)) => (None, Some((path, Some(err)))),
            Ok((path, _)) => (None, Some((path, None))),
        }).unzip();
    let reports: Vec<miette::Report> = reports.into_iter().filter_map(|report| report).collect();
    let io_status: Vec<(PathBuf, Option<std::io::Error>)> = io_status.into_iter().filter_map(|io| io).collect();

    let mut len_reports = reports.len();
    let reports: Vec<_> = reports.into_iter().take(args.max_err).collect();

    if len_reports == 0 {
        return Ok(());
    }

    let io_fails = io_status.iter().fold(0, |fails, (_, err)| if let None = err { fails + 1} else { fails });
    let hidden_len_reports = len_reports - reports.len();
    Err(NonEmpty::from_vec(
    if io_fails > 0 {
        len_reports += 1;
        Some(miette!{
            help = "Please ensure that all nodes in the path have proper permissions.",
            "{io_fails} unsucceeded file read attempts!",
        })
    } else {
        None
    }.into_iter().chain(reports.into_iter().chain((0..1).into_iter().map(|_| miette!{
        severity= Severity::Advice,
        "Totally {len_reports} error(s) with {hidden_len_reports} hidden. You may configure this with --max-err"
    }))).collect()).expect("bro triggered forbidden magic"))
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
