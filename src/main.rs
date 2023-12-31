use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use doter::keymap;
use miette::{miette, Diagnostic, ErrReport, LabeledSpan, NamedSource, Severity};
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
    let args_raw: Vec<String> = std::env::args().collect();
    let args_cmd = Args::command();
    let mut matches = match args_cmd
        .clone()
        .try_get_matches_from_mut(std::env::args_os())
    {
        Ok(matches) => matches,
        Err(err) => return Err(vec![miette!("{err}")]),
    };
    let args = Args::from_arg_matches_mut(&mut matches).expect("bro uses a chinese off brand 克拉普");

    let file_ext_keymap = Some("sublime-keymap".as_ref());
    let keymaps = keymaps_parse(args.packages.as_path(), file_ext_keymap);
    let path_to_real = |path: PathBuf| -> PathBuf {
        // match std::fs::canonicalize(&path) {
        //     Ok(path) => path,
        //     Err(err) if args.verbose => {
        //         let (utf8, _) = path.to_str_idc();
        //         eprintln!("[Verbose log] std::fs::canonicalize({utf8}): {err}");
        //         path
        //     },
        //     _ => path,
        // }

        path
    };

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
                    Error::Syntax {line, col, ..} => ("Invalid syntax! (we tried our best but was your HJSON made in China?)", Some((line, col, "Error occurred nearby"))),
                    Error::Serde {line, col, message: msg, ..} => ("Invalid data!", Some((line, col, msg))),
                    _ => ("", None),
                };
                let labels: Vec<LabeledSpan> = details.into_iter().map(|(line, col, msg)| {
                    let offset = if line > &0 {
                        raw.lines().take(line-1).map(|raw_line| raw_line.len() + 1).sum::<usize>() + col - 1
                    } else { 0 };
                    LabeledSpan::at_offset(offset, msg)
                }).collect();
                let len_raw = raw.len();
                let path = path_to_real(path);
                (Some(miette!{
                    labels = labels,
                    help = format!("File consists of {len_raw} byte(s)"),
                    "{heading}",
                }.with_source_code(NamedSource::new(path.to_str_idc().0, raw))), Some((path, None)))
            },
            Err(InitError::Io(path, err)) => (None, Some((path, Some(err)))),
            Ok((_, KeymapsParse::Skipped)) => (None, None),
            Ok((path, _)) => (None, Some((path, None))),
        }).unzip();
    let reports: Vec<miette::Report> = reports.into_iter().filter_map(|report| report).collect();
    type IoStatus = Vec<(PathBuf, Option<std::io::Error>)>;
    let io_status: IoStatus = io_status.into_iter().filter_map(|io| io).collect();

    let mut len_reports = reports.len();
    let reports: Vec<_> = reports.into_iter().take(args.max_err).collect();

    if len_reports == 0 {
        return Ok(());
    }

    let io_fails = io_status.iter().fold(
        0,
        |fails, (_, err)| if let None = err { fails } else { fails + 1 },
    );
    let hidden_len_reports = len_reports - reports.len();
    Err(if io_fails > -1 {
        let root = args.packages;
        let paths: Vec<&Path> = io_status.iter().map(|(path, _)| path.as_path()).collect();
        let lines = tree::from(canonicalize(&root).unwrap_or(root).as_path(), paths);
        panic!("{}", lines.join("\n"));

        len_reports += 1;
        Some(miette!{
            help = "Please ensure that all nodes in the path have proper permissions.",
            labels = vec![LabeledSpan::at_offset(0, "")],
            "{io_fails} unsucceeded file read attempts!",
        }.with_source_code(lines.join("\n")))
    } else {
        None
    }.into_iter().chain(reports.into_iter().chain((0..1).into_iter().map(|_| miette!{
        severity= Severity::Advice,
        "Totally {len_reports} error(s) with {hidden_len_reports} hidden. You may configure this with --max-err"
    }))).collect())
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

fn keymaps_parse<'a>(
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
                    let bytes = &std::fs::read(path).map_err(|err| InitError::Io(path.to_path_buf(), err))?;
                    let raw = String::from_utf8_lossy(bytes);

                    let parsed = deser_hjson::from_str::<Vec<keymap::KeymapEntry>>(&raw)
                        .map_err(|err| InitError::Parse(path.to_path_buf(), err, raw.to_string()))?;
                    KeymapsParse::Parsed(parsed)
                } else {
                    KeymapsParse::Skipped
                }
            }))
        })
        .collect()
}

/// https://stackoverflow.com/a/60498568/13787084
mod tree {
    use std::{path::Path, ffi::{OsStr, OsString}};
    // A recursive type to represent a directory tree.
    // Simplification: If it has children, it is considered
    // a directory, else considered a file.
    #[derive(Debug, Clone)]
    struct Dir {
        name: OsString,
        children: Vec<Box<Dir>>,
    }

    impl Dir {
        fn new(name: OsString) -> Self {
            Dir {
                name,
                children: Vec::<Box<Self>>::new(),
            }
        }

        fn find_child(&mut self, name: &OsStr) -> Option<&mut Self> {
            for c in self.children.iter_mut() {
                if c.name == name {
                    return Some(c);
                }
            }
            None
        }

        fn add_child(& mut self, leaf: Self) -> &mut Self
        {
            self.children.push(Box::new(leaf));
            self
        }
    }

    /// Form our INPUT:  a list of paths.
    pub fn from(root: &Path, paths: Vec<&Path>) -> Vec<String> {
        // Transformation:
        // A recursive algorithm that converts the list of paths
        // above to Dir (tree) below.
        // ie: paths --> dir
        let mut top = Dir{name: root.into(), children: vec![]};
        for path in paths.into_iter() {
            build_tree(&mut top, &path, 0);
        }

        // Output:  textual `tree` format
        print_dir(&top, 0)
    }

    // fn build_tree(node: &mut Dir, parts: &Vec<String>, depth: usize) {
    fn build_tree(node: &mut Dir, parts: &Path, depth: usize) {
            let item = if let Some(x) = parts.iter().nth(depth) { x } else { return };

            let mut dir = match node.find_child(&item) {
                Some(d) => d,
                None => {
                    let d = Dir::new(item.into());
                    node.add_child(d);
                    match node.find_child(&item) {
                        Some(d2) => d2,
                        None => panic!("Got here!"),
                    }
                }
            };
            build_tree(&mut dir, parts, depth + 1);
    }

    // A function to print a Dir in format similar to unix `tree` command.
    fn print_dir(dir: &Dir, depth: u32) -> Vec<String> {
        let iter = vec![if depth == 0 {
            format!("{}", dir.name.to_string_lossy())
        } else {
            format!(
                "{:indent$}{} {}",
                "",
                "└──",
                dir.name.to_string_lossy(),
                indent = ((depth as usize) - 1) * 4
                )
        }].into_iter();

        let iter2 = dir.children.iter().flat_map(|child| print_dir(child, depth + 1));

        iter.chain(iter2).collect()
    }
}