use std::env;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use walkdir::WalkDir;
use zip::{ZipArchive, ZipWriter};

const DIRS_TO_PROCESS: [&str; 2] = ["main", "iw4x"];
const REMOVABLE_PATTERNS: [&str; 3] = ["images", "sound", "video"];
const REMOVABLE_EXTENSIONS: [&str; 2] = ["iwi", "mp3"];

fn main() -> io::Result<()> {
    let base_dir = env::args()
        .nth(1)
        .map(|p| Path::new(&p).to_owned())
        .unwrap_or_else(|| Path::new(".").to_owned());

    if !base_dir.exists() {
        println!("Directory '{}' not found", base_dir.display());
        return Ok(());
    }

    let mut total_files_removed = 0;
    let mut total_bytes_removed = 0;

    for dir_name in DIRS_TO_PROCESS {
        let (files, bytes) = process_directory(&base_dir, dir_name)?;
        total_files_removed += files;
        total_bytes_removed += bytes;
    }

    println!("\nTotal files removed: {}", total_files_removed);
    println!(
        "Total size removed: {:.2} MB",
        total_bytes_removed as f64 / 1_048_576.0
    );
    Ok(())
}

fn process_directory(base_dir: &Path, dir_name: &str) -> io::Result<(u32, u64)> {
    let work_dir = base_dir.join(dir_name);
    if !work_dir.exists() {
        println!("Directory '{}' not found, skipping...", work_dir.display());
        return Ok((0, 0));
    }

    println!("\nProcessing directory: {}", work_dir.display());

    let mut files_removed = 0;
    let mut bytes_removed = 0;

    let video_dir = work_dir.join("video");
    if video_dir.exists() {
        let video_size = get_dir_size(&video_dir)?;
        println!(
            "Removing video directory ({:.2} MB)...",
            video_size as f64 / 1_048_576.0
        );
        fs::remove_dir_all(&video_dir)?;
        bytes_removed += video_size;
    }

    for entry in WalkDir::new(&work_dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "iwd") {
            println!("Processing: {}", path.display());
            match process_iwd_file(path) {
                Ok((files, bytes)) => {
                    files_removed += files;
                    bytes_removed += bytes;
                }
                Err(e) => println!("Error processing {}: {}", path.display(), e),
            }
        }
    }

    Ok((files_removed, bytes_removed))
}

fn get_dir_size(path: &Path) -> io::Result<u64> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .try_fold(0, |acc, entry| -> io::Result<u64> {
            Ok(acc + entry.metadata()?.len())
        })
}

fn should_remove_file(name: &str) -> bool {
    let path = Path::new(name);
    REMOVABLE_PATTERNS
        .iter()
        .any(|&pattern| path.starts_with(pattern))
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| REMOVABLE_EXTENSIONS.contains(&ext))
}

fn process_iwd_file(path: &Path) -> io::Result<(u32, u64)> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut files_to_remove = Vec::new();
    let mut bytes_to_remove = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if should_remove_file(file.name()) {
            files_to_remove.push(i);
            bytes_to_remove += file.size();
        }
    }

    if files_to_remove.is_empty() {
        return Ok((0, 0));
    }

    let temp_path = path.with_extension("iwd.temp");
    let temp_file = File::create(&temp_path)?;
    let mut zip_writer = ZipWriter::new(temp_file);

    for i in 0..archive.len() {
        if !files_to_remove.contains(&i) {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();
            if let Err(e) = zip_writer.raw_copy_file(file) {
                println!("Failed to copy {}: {}", name, e);
            }
        }
    }

    zip_writer.finish()?;
    fs::remove_file(path)?;
    fs::rename(temp_path, path)?;

    println!(
        "Removed {} files ({:.2} MB) from {}",
        files_to_remove.len(),
        bytes_to_remove as f64 / 1_048_576.0,
        path.display()
    );

    Ok((files_to_remove.len() as u32, bytes_to_remove))
}
