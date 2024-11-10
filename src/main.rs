use std::env;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use walkdir::WalkDir;
use zip::{ZipArchive, ZipWriter};

const DIRS_TO_PROCESS: [&str; 2] = ["main", "iw4x"];
const REMOVABLE_PATTERNS: [&str; 3] = ["images", "sound", "video"];
const REMOVABLE_EXTENSIONS: [&str; 2] = ["iwi", "mp3"];
const MB_DIVISOR: f64 = 1_048_576.0;

struct ProcessingStats {
    files_removed: u32,
    bytes_removed: u64,
}

impl ProcessingStats {
    fn new() -> Self {
        Self {
            files_removed: 0,
            bytes_removed: 0,
        }
    }

    fn add(&mut self, files: u32, bytes: u64) {
        self.files_removed += files;
        self.bytes_removed += bytes;
    }

    fn display_total(&self) {
        println!("\nTotal files removed: {}", self.files_removed);
        println!(
            "Total size removed: {:.2} MB",
            self.bytes_removed as f64 / MB_DIVISOR
        );
    }
}

fn main() -> io::Result<()> {
    let base_dir = env::args()
        .nth(1)
        .map(|p| Path::new(&p).to_owned())
        .unwrap_or_else(|| Path::new(".").to_owned());

    if !base_dir.exists() {
        println!("Directory '{}' not found", base_dir.display());
        return Ok(());
    }

    let mut stats = ProcessingStats::new();

    for dir_name in DIRS_TO_PROCESS {
        process_directory(&base_dir, dir_name, &mut stats)?;
    }

    stats.display_total();
    Ok(())
}

fn process_directory(base_dir: &Path, dir_name: &str, stats: &mut ProcessingStats) -> io::Result<()> {
    let work_dir = base_dir.join(dir_name);
    if !work_dir.exists() {
        println!("Directory '{}' not found, skipping...", work_dir.display());
        return Ok(());
    }

    println!("\nProcessing directory: {}", work_dir.display());

    if let Some(video_bytes) = process_video_directory(&work_dir)? {
        stats.add(0, video_bytes);
    }

    process_iwd_files(&work_dir, stats)?;

    Ok(())
}

fn process_video_directory(work_dir: &Path) -> io::Result<Option<u64>> {
    let video_dir = work_dir.join("video");
    if !video_dir.exists() {
        return Ok(None);
    }

    let video_size = get_dir_size(&video_dir)?;
    println!(
        "Removing video directory ({:.2} MB)...",
        video_size as f64 / MB_DIVISOR
    );
    fs::remove_dir_all(&video_dir)?;
    Ok(Some(video_size))
}

fn process_iwd_files(work_dir: &Path, stats: &mut ProcessingStats) -> io::Result<()> {
    for entry in WalkDir::new(work_dir).min_depth(1).max_depth(1) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "iwd") {
            println!("Processing: {}", path.display());
            match process_iwd_file(path) {
                Ok((files, bytes)) => stats.add(files, bytes),
                Err(e) => println!("Error processing {}: {}", path.display(), e),
            }
        }
    }
    Ok(())
}

fn get_dir_size(path: &Path) -> io::Result<u64> {
    let mut total_size = 0;
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        total_size += entry.metadata()?.len();
    }
    Ok(total_size)
}

fn should_remove_file(name: &str) -> bool {
    let path = Path::new(name);
    REMOVABLE_PATTERNS
        .iter()
        .any(|&pattern| path.starts_with(pattern))
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .map_or(false, |ext| REMOVABLE_EXTENSIONS.contains(&ext))
}

fn process_iwd_file(path: &Path) -> io::Result<(u32, u64)> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let total_files = archive.len();

    let (files_to_remove, bytes_to_remove) = analyze_archive(&mut archive)?;
    if files_to_remove.is_empty() {
        return Ok((0, 0));
    }

    create_filtered_archive(path, &mut archive, &files_to_remove, total_files)?;

    println!(
        "Removed {} files ({:.2} MB) from {}",
        files_to_remove.len(),
        bytes_to_remove as f64 / MB_DIVISOR,
        path.display()
    );

    Ok((files_to_remove.len() as u32, bytes_to_remove))
}

fn analyze_archive(archive: &mut ZipArchive<File>) -> io::Result<(Vec<usize>, u64)> {
    let mut files_to_remove = Vec::new();
    let mut bytes_to_remove = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if should_remove_file(file.name()) {
            files_to_remove.push(i);
            bytes_to_remove += file.size();
        }
    }

    Ok((files_to_remove, bytes_to_remove))
}

fn create_filtered_archive(
    path: &Path,
    archive: &mut ZipArchive<File>,
    files_to_remove: &[usize],
    total_files: usize,
) -> io::Result<()> {
    let temp_path = path.with_extension("iwd.temp");
    let temp_file = File::create(&temp_path)?;
    let mut zip_writer = ZipWriter::new(temp_file);

    for i in 0..total_files {
        if files_to_remove.contains(&i) {
            continue;
        }

        let file = archive.by_index(i)?;
        let name = file.name().to_string();

        if let Err(e) = zip_writer.raw_copy_file(file) {
            println!("Failed to copy {}: {}", name, e);
        }
    }

    zip_writer.finish()?;
    fs::remove_file(path)?;
    fs::rename(temp_path, path)?;

    Ok(())
}
