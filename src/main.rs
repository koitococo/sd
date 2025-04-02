mod cli;
mod error;
mod input;

pub(crate) mod replacer;

use clap::Parser;
use memmap2::{Mmap, MmapMut};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use std::{
    fs,
    io::{stdout, Write},
    ops::DerefMut,
    path::PathBuf,
    process,
};

pub(crate) use self::error::{Error, FailedJobs, Result};
pub(crate) use self::input::Source;
use self::input::{make_mmap, make_mmap_stdin};
use self::replacer::{FancyReplacer, RegexReplacer, Replacer};

fn main() {
    if let Err(e) = try_main() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let options = cli::Options::parse();

    if options.literal_mode && !options.replace_with.is_empty() {
        eprintln!("error: -F and -R are mutually exclusive");
        process::exit(1);
    }

    if options.replace_with.is_empty() {
        eprintln!("error: no replacement string provided");
        process::exit(1);
    }

    if options.use_fancy_regex {
        fancy_main(options)
    } else {
        regex_main(options)
    }
}

fn fancy_main(options: cli::Options) -> Result<()> {
    let replacer = FancyReplacer::new(
        options.find,
        options.replace_with,
        options.literal_mode,
        options.flags,
        options.replacements,
    )?;

    let sources = if options.files.is_empty() {
        Source::from_stdin()
    } else {
        Source::from_paths(options.files)
    };

    let mmaps: Vec<Mmap> = sources
        .iter()
        .filter_map(|source| match source {
            Source::File(path) => {
                if path.exists() {
                    unsafe { make_mmap(path).ok() }
                } else {
                    eprintln!("{}", Error::InvalidPath(path.to_owned()));
                    None
                }
            }
            Source::Stdin => make_mmap_stdin().ok(),
        })
        .collect();

    let replaced: Vec<_> = {
        mmaps
            .par_iter()
            .filter_map(|mmap| {
                if mmap.len() > 1024 * 1024 {
                    eprintln!("error: file is TOO LARGE! Currently we need to copy the whole file to memory, and may cause performance issues.");
                }
                let content = unsafe {str::from_utf8_unchecked(&mmap)};
                replacer.replace(content, options.only_matched, options.use_color)
            })
            .collect()
    };

    if options.preview || sources.first() == Some(&Source::Stdin) {
        let mut handle = stdout().lock();

        for (source, replaced) in sources.iter().zip(replaced) {
            if sources.len() > 1 {
                writeln!(handle, "----- {} -----", source.display())?;
            }
            handle.write_all(replaced.as_bytes())?;
        }
    } else {
        // Windows requires closing mmap before writing:
        // > The requested operation cannot be performed on a file with a user-mapped section open
        #[cfg(target_family = "windows")]
        let replaced: Vec<Vec<u8>> =
            replaced.into_iter().map(|r| r.to_vec()).collect();
        #[cfg(target_family = "windows")]
        drop(mmaps);

        let mut failed_jobs = Vec::new();
        for (source, replaced) in sources.iter().zip(replaced) {
            match source {
                Source::File(path) => {
                    if let Err(e) = write_with_temp(path, replaced.as_bytes()) {
                        failed_jobs.push((path.to_owned(), e));
                    }
                }
                _ => unreachable!("stdin should go previous branch"),
            }
        }
        if !failed_jobs.is_empty() {
            return Err(Error::FailedJobs(FailedJobs(failed_jobs)));
        }
    }

    Ok(())
}

fn regex_main(options: cli::Options) -> Result<()> {
    let replacer = RegexReplacer::new(
        options.find,
        options.replace_with,
        options.literal_mode,
        options.flags,
        options.replacements,
    )?;

    let sources = if options.files.is_empty() {
        Source::from_stdin()
    } else {
        Source::from_paths(options.files)
    };

    let mmaps: Vec<Mmap> = sources
        .iter()
        .filter_map(|source| match source {
            Source::File(path) => {
                if path.exists() {
                    unsafe { make_mmap(path).ok() }
                } else {
                    eprintln!("{}", Error::InvalidPath(path.to_owned()));
                    None
                }
            }
            Source::Stdin => make_mmap_stdin().ok(),
        })
        .collect();

    let replaced: Vec<_> = mmaps
        .par_iter()
        .filter_map(|mmap| {
            replacer.replace(&mmap, options.only_matched, options.use_color)
        })
        .collect();

    if options.preview || sources.first() == Some(&Source::Stdin) {
        let mut handle = stdout().lock();

        for (source, replaced) in sources.iter().zip(replaced) {
            if sources.len() > 1 {
                writeln!(handle, "----- {} -----", source.display())?;
            }
            handle.write_all(&replaced)?;
        }
    } else {
        // Windows requires closing mmap before writing:
        // > The requested operation cannot be performed on a file with a user-mapped section open
        #[cfg(target_family = "windows")]
        let replaced: Vec<Vec<u8>> =
            replaced.into_iter().map(|r| r.to_vec()).collect();
        #[cfg(target_family = "windows")]
        drop(mmaps);

        let mut failed_jobs = Vec::new();
        for (source, replaced) in sources.iter().zip(replaced) {
            match source {
                Source::File(path) => {
                    if let Err(e) = write_with_temp(path, &replaced) {
                        failed_jobs.push((path.to_owned(), e));
                    }
                }
                _ => unreachable!("stdin should go previous branch"),
            }
        }
        if !failed_jobs.is_empty() {
            return Err(Error::FailedJobs(FailedJobs(failed_jobs)));
        }
    }

    Ok(())
}

fn write_with_temp(path: &PathBuf, data: &[u8]) -> Result<()> {
    let path = fs::canonicalize(path)?;

    let temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| Error::InvalidPath(path.to_path_buf()))?,
    )?;

    let file = temp.as_file();
    file.set_len(data.len() as u64)?;
    if let Ok(metadata) = fs::metadata(&path) {
        file.set_permissions(metadata.permissions()).ok();
    }

    if !data.is_empty() {
        let mut mmap_temp = unsafe { MmapMut::map_mut(file)? };
        mmap_temp.deref_mut().write_all(data)?;
        mmap_temp.flush_async()?;
    }

    temp.persist(&path)?;

    Ok(())
}
