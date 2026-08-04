use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use clap::Parser;
use colored::*;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "dupfind", version, about = "Find duplicate files using SHA-256", author = "fantasywastaken")]
struct Args {
    #[arg(default_value = ".")]
    path: PathBuf,

    #[arg(long, default_value = "1")]
    min_size: String,

    #[arg(long)]
    delete_interactive: bool,
}

fn parse_size(s: &str) -> u64 {
    let up = s.trim().to_uppercase();
    let (num, mult) = if let Some(n) = up.strip_suffix("GB") {
        (n, 1_073_741_824u64)
    } else if let Some(n) = up.strip_suffix("MB") {
        (n, 1_048_576u64)
    } else if let Some(n) = up.strip_suffix("KB") {
        (n, 1024u64)
    } else if let Some(n) = up.strip_suffix('B') {
        (n, 1u64)
    } else {
        (up.as_str(), 1u64)
    };
    num.trim().parse::<u64>().unwrap_or(0).saturating_mul(mult)
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < 4 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, UNITS[unit])
}

fn hash_file(path: &PathBuf) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let out = hasher.finalize();
    let hex: String = out.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(hex)
}

fn main() -> Result<()> {
    #[cfg(windows)]
    let _ = colored::control::set_virtual_terminal(true);

    let args = Args::parse();
    let min = parse_size(&args.min_size);

    eprintln!("{} scanning {}", "==>".cyan().bold(), args.path.display().to_string().white());
    eprintln!("{} minimum size: {}", "==>".cyan().bold(), format_size(min).yellow());

    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut file_count: u64 = 0;

    for entry in WalkDir::new(&args.path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                if size >= min {
                    by_size.entry(size).or_default().push(entry.path().to_path_buf());
                    file_count += 1;
                    if file_count % 1000 == 0 {
                        eprint!("\r{} indexed {} files...", "==>".cyan(), file_count);
                    }
                }
            }
        }
    }
    eprintln!("\r{} indexed {} files                        ", "==>".cyan().bold(), file_count.to_string().green());

    let candidates: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter()
        .filter(|(_, v)| v.len() > 1)
        .collect();

    let total_groups = candidates.len();
    eprintln!("{} hashing {} size-groups", "==>".cyan().bold(), total_groups.to_string().yellow());

    let dup_groups: Mutex<Vec<(u64, String, Vec<PathBuf>)>> = Mutex::new(Vec::new());
    let processed = AtomicUsize::new(0);

    candidates.par_iter().for_each(|(size, paths)| {
        let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths {
            if let Ok(h) = hash_file(p) {
                by_hash.entry(h).or_default().push(p.clone());
            }
        }
        {
            let mut guard = dup_groups.lock().unwrap();
            for (h, group) in by_hash {
                if group.len() > 1 {
                    guard.push((*size, h, group));
                }
            }
        }
        let p = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if p % 25 == 0 || p == total_groups {
            eprint!("\r{} hashed {}/{} groups", "==>".cyan(), p, total_groups);
        }
    });
    eprintln!();

    let mut groups = dup_groups.into_inner().unwrap();
    groups.sort_by(|a, b| b.0.cmp(&a.0));

    if groups.is_empty() {
        println!("{} no duplicate files found", "==>".green().bold());
        return Ok(());
    }

    let mut total_wasted: u64 = 0;
    for (i, (size, hash, files)) in groups.iter().enumerate() {
        println!();
        println!("{} Group #{}  size={}  sha256={}",
            "==>".yellow().bold(),
            (i + 1).to_string().yellow().bold(),
            format_size(*size).cyan(),
            hash[..16].dimmed());
        for f in files {
            println!("    {}", f.display());
        }
        total_wasted = total_wasted.saturating_add(size.saturating_mul(files.len() as u64 - 1));
    }

    println!();
    println!("{} total duplicate groups: {}", "==>".green().bold(), groups.len().to_string().green());
    println!("{} total wasted space:     {}", "==>".red().bold(), format_size(total_wasted).red().bold());

    if args.delete_interactive {
        use std::io::{self, BufRead, Write};
        let stdin = io::stdin();
        println!();
        println!("{} entering interactive delete mode", "==>".magenta().bold());
        for (i, (_, _, files)) in groups.iter().enumerate() {
            println!();
            println!("{} Group #{}:", "==>".yellow().bold(), i + 1);
            for (j, f) in files.iter().enumerate() {
                println!("  [{}] {}", (j + 1).to_string().cyan(), f.display());
            }
            print!("{} keep number (s=skip, q=quit): ", "?".magenta().bold());
            io::stdout().flush()?;
            let mut line = String::new();
            stdin.lock().read_line(&mut line)?;
            let ans = line.trim();
            if ans == "q" { break; }
            if ans == "s" || ans.is_empty() { continue; }
            if let Ok(keep) = ans.parse::<usize>() {
                if keep >= 1 && keep <= files.len() {
                    for (j, f) in files.iter().enumerate() {
                        if j + 1 != keep {
                            match std::fs::remove_file(f) {
                                Ok(_) => println!("  {} deleted {}", "-".red().bold(), f.display()),
                                Err(e) => println!("  {} failed on {}: {}", "!".red().bold(), f.display(), e),
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
