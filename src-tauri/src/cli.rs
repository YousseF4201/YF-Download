use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use yf_download_lib::core::{
    DownloadConfig, DownloadEngine, MIN_SEGMENT_THRESHOLD, NetworkClient, ProgressUpdate,
};
use yf_download_lib::db::Database;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOptions {
    pub url: Option<String>,
    pub destination: PathBuf,
    pub segments: usize,
    pub resume_id: Option<i64>,
    pub resume_flag: bool,
    pub db_path: Option<PathBuf>,
    pub expected_sha256: Option<String>,
    pub list_tasks: bool,
    pub show_help: bool,
}

pub fn parse_cli_args(args: &[String]) -> Result<CliOptions, String> {
    let mut opts = CliOptions {
        url: None,
        destination: PathBuf::from("."),
        segments: 8,
        resume_id: None,
        resume_flag: false,
        db_path: None,
        expected_sha256: None,
        list_tasks: false,
        show_help: false,
    };

    let mut i = 1;
    let mut positional = Vec::new();

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                opts.show_help = true;
                return Ok(opts);
            }
            "-l" | "--list" => {
                opts.list_tasks = true;
                i += 1;
            }
            "-s" | "--segments" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --segments".into());
                }
                let seg = args[i].parse::<usize>().map_err(|_| {
                    format!(
                        "Invalid number of segments: '{}'. Must be a positive integer.",
                        args[i]
                    )
                })?;
                if seg == 0 {
                    return Err("Number of segments must be at least 1".into());
                }
                opts.segments = seg;
                i += 1;
            }
            "-r" | "--resume" => {
                opts.resume_flag = true;
                i += 1;
                if i < args.len() && !args[i].starts_with('-') {
                    if let Ok(id) = args[i].parse::<i64>() {
                        opts.resume_id = Some(id);
                        i += 1;
                    }
                }
            }
            "--db" => {
                i += 1;
                if i >= args.len() || args[i].starts_with('-') {
                    opts.db_path = Some(PathBuf::from("yf_downloads.db"));
                } else {
                    opts.db_path = Some(PathBuf::from(&args[i]));
                    i += 1;
                }
            }
            "--sha256" => {
                i += 1;
                if i >= args.len() {
                    return Err("Missing value for --sha256".into());
                }
                opts.expected_sha256 = Some(args[i].clone());
                i += 1;
            }
            val if val.starts_with('-') => {
                return Err(format!(
                    "Unknown option: '{val}'. Run with --help for usage."
                ));
            }
            val => {
                positional.push(val.to_string());
                i += 1;
            }
        }
    }

    if !positional.is_empty() {
        opts.url = Some(positional[0].clone());
    }
    if positional.len() > 1 {
        opts.destination = PathBuf::from(&positional[1]);
    }

    Ok(opts)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn print_progress(p: &ProgressUpdate) {
    let bar_width: usize = 30;
    let filled = if p.progress_percent > 0.0 {
        ((p.progress_percent / 100.0) * bar_width as f64) as usize
    } else {
        0
    };
    let empty = bar_width.saturating_sub(filled);

    let eta_str = match p.eta_seconds {
        Some(s) if s > 0 => format!("ETA: {}", format_duration(s)),
        Some(_) => "ETA: done".to_string(),
        None => "ETA: --".to_string(),
    };

    let size_str = match p.total_bytes {
        Some(total) => format!(
            "{} / {}",
            format_bytes(p.downloaded_bytes),
            format_bytes(total)
        ),
        None => format_bytes(p.downloaded_bytes),
    };

    print!(
        "\r  [{}{}] {:.1}%  {}  {}/s  {}",
        "█".repeat(filled),
        "░".repeat(empty),
        p.progress_percent,
        size_str,
        format_bytes(p.speed_bytes_per_sec),
        eta_str,
    );
    let _ = std::io::stdout().flush();
}

fn print_usage() {
    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!("║             YF Download — CLI Test Tool                  ║");
    eprintln!("║             v0.3.0 (Phase 2 - Multi-Segment)             ║");
    eprintln!("╚══════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  yf-cli <URL> [destination] [options]");
    eprintln!("  yf-cli --resume <ID> [--db <path>]");
    eprintln!("  yf-cli --list [--db <path>]");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  yf-cli https://example.com/large.zip");
    eprintln!("  yf-cli https://example.com/large.zip ./downloads/ --segments 16");
    eprintln!("  yf-cli https://example.com/large.zip --resume --db my_downloads.db");
    eprintln!("  yf-cli --resume 1 --db my_downloads.db");
    eprintln!("  yf-cli --list --db my_downloads.db");
    eprintln!("  yf-cli https://example.com/file.zip --sha256 abc123def456...");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -s, --segments <N>   Number of concurrent download segments (default: 8)");
    eprintln!("  -r, --resume [ID]    Resume an existing download by database ID or URL");
    eprintln!("      --db [PATH]      Enable SQLite state persistence (default: yf_downloads.db)");
    eprintln!("      --sha256 <HASH>  Verify the downloaded file's SHA-256 checksum");
    eprintln!("  -l, --list           List downloads recorded in the SQLite database");
    eprintln!("  -h, --help           Display this help information");
}

#[tokio::main]
async fn main() {
    let raw_args: Vec<String> = std::env::args().collect();

    let opts = match parse_cli_args(&raw_args) {
        Ok(o) => o,
        Err(err) => {
            eprintln!("Error: {err}");
            eprintln!("Run 'yf-cli --help' for usage instructions.");
            std::process::exit(1);
        }
    };

    if opts.show_help || (raw_args.len() < 2 && opts.url.is_none()) {
        print_usage();
        std::process::exit(if opts.show_help { 0 } else { 1 });
    }

    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!("║             YF Download — CLI Test Tool                  ║");
    eprintln!("╚══════════════════════════════════════════════════════════╝");
    eprintln!();

    // Mode 1: List recorded tasks from SQLite
    if opts.list_tasks {
        let db_file = opts
            .db_path
            .unwrap_or_else(|| PathBuf::from("yf_downloads.db"));
        if !db_file.exists() {
            eprintln!("✗ Database file not found: {}", db_file.display());
            std::process::exit(1);
        }
        let db = match Database::open(&db_file) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("✗ Failed to open database {}: {e}", db_file.display());
                std::process::exit(1);
            }
        };

        match db.list_downloads() {
            Ok(downloads) if downloads.is_empty() => {
                println!(
                    "No download tasks recorded in database ({}).",
                    db_file.display()
                );
            }
            Ok(downloads) => {
                println!("Downloads in {}:", db_file.display());
                println!(
                    "{:<5} | {:<24} | {:<10} | {:<18} | {}",
                    "ID", "File", "Status", "Progress", "URL"
                );
                println!("{}", "─".repeat(80));
                for dl in downloads {
                    let total = dl
                        .total_size
                        .map(format_bytes)
                        .unwrap_or_else(|| "?".into());
                    let prog = format!("{} / {}", format_bytes(dl.downloaded_bytes), total);
                    println!(
                        "{:<5} | {:<24} | {:<10?} | {:<18} | {}",
                        dl.id, dl.file_name, dl.status, prog, dl.url
                    );
                }
            }
            Err(e) => {
                eprintln!("✗ Failed to query downloads: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Mode 2: Resume task directly by database ID
    if let Some(task_id) = opts.resume_id {
        let db_file = opts
            .db_path
            .unwrap_or_else(|| PathBuf::from("yf_downloads.db"));
        let db = match Database::open(&db_file) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("✗ Failed to open database {}: {e}", db_file.display());
                std::process::exit(1);
            }
        };

        let record = match db.get_download(task_id) {
            Ok(Some(r)) => r,
            Ok(None) => {
                eprintln!(
                    "✗ Download task #{task_id} not found in database {}",
                    db_file.display()
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("✗ Failed to query task #{task_id}: {e}");
                std::process::exit(1);
            }
        };

        eprintln!(
            "⟳ Resuming download task #{} ({})",
            task_id, record.file_name
        );
        eprintln!("  ✓ URL: {}", record.url);
        eprintln!("  ✓ Destination: {}", record.save_path);
        eprintln!("  ✓ Status: {:?}", record.status);
        if let Some(total) = record.total_size {
            eprintln!(
                "  ✓ Existing progress: {} / {}",
                format_bytes(record.downloaded_bytes),
                format_bytes(total)
            );
        }
        eprintln!("  ✓ Segments configured: {}", opts.segments);
        eprintln!("  ✓ Database persistence: {}", db_file.display());
        eprintln!();

        let client =
            NetworkClient::new(Duration::from_secs(30)).expect("Failed to create network client");
        let config = DownloadConfig::default().with_segments(opts.segments);
        let engine = DownloadEngine::with_db(client, config, db);

        eprintln!("⟳ Resuming download...");
        match engine
            .resume_download(task_id, opts.expected_sha256.as_deref(), move |progress| {
                print_progress(&progress);
            })
            .await
        {
            Ok(result) => {
                println!();
                eprintln!();
                eprintln!("═══════════════════════════════════════════════");
                eprintln!("  ✓ Download resumed and completed successfully!");
                eprintln!("  ✓ File: {}", result.target_path.display());
                eprintln!("  ✓ Size: {}", format_bytes(result.bytes_downloaded));
                eprintln!("  ✓ SHA-256: {}", result.sha256_hash);
                eprintln!("  ✓ Duration: {:.2}s", result.duration.as_secs_f64());
                if result.duration.as_secs_f64() > 0.0 {
                    let avg_speed = result.bytes_downloaded as f64 / result.duration.as_secs_f64();
                    eprintln!("  ✓ Average speed: {}/s", format_bytes(avg_speed as u64));
                }
                eprintln!("  ✓ Segments used: {}", result.segments_used);
                if opts.expected_sha256.is_some() {
                    eprintln!("  ✓ Checksum verification: PASSED ✓");
                }
                eprintln!("═══════════════════════════════════════════════");
            }
            Err(e) => {
                println!();
                eprintln!();
                eprintln!("═══════════════════════════════════════════════");
                eprintln!("  ✗ Resumed download failed: {e}");
                eprintln!("═══════════════════════════════════════════════");
                std::process::exit(1);
            }
        }
        return;
    }

    // Mode 3: Normal or URL-based download (with optional --segments, --db, --resume)
    let url = match opts.url {
        Some(ref u) => u,
        None => {
            eprintln!("Error: URL is required.");
            print_usage();
            std::process::exit(1);
        }
    };

    // 1. Query file metadata
    eprintln!("⟳ Querying file metadata...");
    let client =
        NetworkClient::new(Duration::from_secs(30)).expect("Failed to create network client");

    let meta = match client.query_metadata(url).await {
        Ok(m) => {
            eprintln!("  ✓ File name: {}", m.file_name);
            if let Some(size) = m.total_size {
                eprintln!("  ✓ File size: {}", format_bytes(size));
            } else {
                eprintln!("  ⚠ File size: unknown");
            }
            eprintln!(
                "  ✓ Range support: {}",
                if m.supports_range { "yes" } else { "no" }
            );
            if let Some(ref ct) = m.content_type {
                eprintln!("  ✓ Content type: {ct}");
            }
            m
        }
        Err(e) => {
            eprintln!("  ✗ Failed to query metadata: {e}");
            std::process::exit(1);
        }
    };

    let can_multi_segment = opts.segments > 1
        && meta.supports_range
        && meta.total_size.is_some_and(|s| s >= MIN_SEGMENT_THRESHOLD);

    if can_multi_segment {
        eprintln!(
            "  ✓ Download strategy: Multi-Segment ({} concurrent connections)",
            opts.segments
        );
    } else if meta.supports_range {
        eprintln!("  ✓ Download strategy: Single-connection (requested 1 segment or file small)");
    } else {
        eprintln!("  ✓ Download strategy: Single-connection (server does not support Range)");
    }

    // 2. Setup SQLite persistence if --db or --resume is enabled
    let maybe_db = if let Some(ref path) = opts.db_path {
        eprintln!("  ✓ SQLite Database: {}", path.display());
        match Database::open(path) {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("  ✗ Failed to open database {}: {e}", path.display());
                std::process::exit(1);
            }
        }
    } else if opts.resume_flag {
        let default_db = PathBuf::from("yf_downloads.db");
        eprintln!(
            "  ✓ SQLite Database: {} (auto-enabled for resume)",
            default_db.display()
        );
        match Database::open(&default_db) {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("  ✗ Failed to open database: {e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    eprintln!();

    // 3. Configure and execute download
    eprintln!("⟳ Starting download...");
    let config = DownloadConfig::default().with_segments(opts.segments);
    let engine = match maybe_db {
        Some(db) => DownloadEngine::with_db(client, config, db),
        None => DownloadEngine::new(client, config),
    };

    let expected_sha256 = opts.expected_sha256.clone();
    match engine
        .download(
            url,
            &opts.destination,
            expected_sha256.as_deref(),
            move |progress| {
                print_progress(&progress);
            },
        )
        .await
    {
        Ok(result) => {
            println!();
            eprintln!();
            eprintln!("═══════════════════════════════════════════════");
            eprintln!("  ✓ Download complete!");
            eprintln!("  ✓ File: {}", result.target_path.display());
            eprintln!("  ✓ Size: {}", format_bytes(result.bytes_downloaded));
            eprintln!("  ✓ SHA-256: {}", result.sha256_hash);
            eprintln!("  ✓ Duration: {:.2}s", result.duration.as_secs_f64());
            if result.duration.as_secs_f64() > 0.0 {
                let avg_speed = result.bytes_downloaded as f64 / result.duration.as_secs_f64();
                eprintln!("  ✓ Average speed: {}/s", format_bytes(avg_speed as u64));
            }
            eprintln!("  ✓ Segments used: {}", result.segments_used);
            if expected_sha256.is_some() {
                eprintln!("  ✓ Checksum verification: PASSED ✓");
            }
            eprintln!("═══════════════════════════════════════════════");
        }
        Err(e) => {
            println!();
            eprintln!();
            eprintln!("═══════════════════════════════════════════════");
            eprintln!("  ✗ Download failed: {e}");
            eprintln!("═══════════════════════════════════════════════");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_args_defaults() {
        let args = vec![
            "yf-cli".to_string(),
            "https://example.com/file.zip".to_string(),
        ];
        let opts = parse_cli_args(&args).unwrap();
        assert_eq!(opts.url.as_deref(), Some("https://example.com/file.zip"));
        assert_eq!(opts.destination, PathBuf::from("."));
        assert_eq!(opts.segments, 8);
        assert!(!opts.resume_flag);
        assert_eq!(opts.resume_id, None);
        assert_eq!(opts.db_path, None);
        assert_eq!(opts.expected_sha256, None);
        assert!(!opts.list_tasks);
    }

    #[test]
    fn test_parse_cli_args_custom_segments_and_dest() {
        let args = vec![
            "yf-cli".to_string(),
            "https://example.com/file.zip".to_string(),
            "/downloads/target.zip".to_string(),
            "--segments".to_string(),
            "16".to_string(),
        ];
        let opts = parse_cli_args(&args).unwrap();
        assert_eq!(opts.url.as_deref(), Some("https://example.com/file.zip"));
        assert_eq!(opts.destination, PathBuf::from("/downloads/target.zip"));
        assert_eq!(opts.segments, 16);
    }

    #[test]
    fn test_parse_cli_args_resume_with_id() {
        let args = vec![
            "yf-cli".to_string(),
            "--resume".to_string(),
            "42".to_string(),
            "--db".to_string(),
            "custom.db".to_string(),
        ];
        let opts = parse_cli_args(&args).unwrap();
        assert!(opts.resume_flag);
        assert_eq!(opts.resume_id, Some(42));
        assert_eq!(opts.db_path, Some(PathBuf::from("custom.db")));
    }

    #[test]
    fn test_parse_cli_args_list_and_db() {
        let args = vec![
            "yf-cli".to_string(),
            "--list".to_string(),
            "--db".to_string(),
            "tasks.db".to_string(),
        ];
        let opts = parse_cli_args(&args).unwrap();
        assert!(opts.list_tasks);
        assert_eq!(opts.db_path, Some(PathBuf::from("tasks.db")));
    }

    #[test]
    fn test_parse_cli_args_invalid_segments() {
        let args = vec![
            "yf-cli".to_string(),
            "https://example.com/file.zip".to_string(),
            "--segments".to_string(),
            "0".to_string(),
        ];
        assert!(parse_cli_args(&args).is_err());
    }

    #[test]
    fn test_format_bytes_formatting() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10.00 MB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.00 GB");
    }

    #[test]
    fn test_format_duration_formatting() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(125), "2m 5s");
        assert_eq!(format_duration(3665), "1h 1m 5s");
    }
}
