use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use yf_download_lib::core::{DownloadConfig, DownloadEngine, NetworkClient, ProgressUpdate};

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
    let bar_width = 30;
    let filled = if p.progress_percent > 0.0 {
        ((p.progress_percent / 100.0) * bar_width as f64) as usize
    } else {
        0
    };
    let empty = bar_width - filled;

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
    eprintln!("╔══════════════════════════════════════════════╗");
    eprintln!("║        YF Download — CLI Test Tool           ║");
    eprintln!("║            v0.1.0 (Phase 1)                  ║");
    eprintln!("╚══════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  yf-cli <URL> [destination] [--sha256 <hash>]");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  yf-cli https://example.com/file.zip");
    eprintln!("  yf-cli https://example.com/file.zip ./downloads/");
    eprintln!("  yf-cli https://example.com/file.zip ./file.zip --sha256 abc123...");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --sha256 <hash>   Verify the downloaded file's SHA-256 checksum");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        std::process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    let url = &args[1];

    let destination = if args.len() >= 3 && !args[2].starts_with("--") {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from(".")
    };

    let expected_sha256 = args
        .windows(2)
        .find(|w| w[0] == "--sha256")
        .map(|w| w[1].clone());

    eprintln!("╔══════════════════════════════════════════════╗");
    eprintln!("║        YF Download — CLI Test Tool           ║");
    eprintln!("╚══════════════════════════════════════════════╝");
    eprintln!();

    // 1. Query metadata
    eprintln!("⟳ Querying file metadata...");
    let client = NetworkClient::new(Duration::from_secs(30)).expect("Failed to create HTTP client");

    match client.query_metadata(url).await {
        Ok(meta) => {
            eprintln!("  ✓ File name: {}", meta.file_name);
            if let Some(size) = meta.total_size {
                eprintln!("  ✓ File size: {}", format_bytes(size));
            } else {
                eprintln!("  ⚠ File size: unknown");
            }
            eprintln!(
                "  ✓ Range support: {}",
                if meta.supports_range { "yes" } else { "no" }
            );
            if let Some(ref ct) = meta.content_type {
                eprintln!("  ✓ Content type: {ct}");
            }
            eprintln!();
        }
        Err(e) => {
            eprintln!("  ✗ Failed to query metadata: {e}");
            std::process::exit(1);
        }
    }

    // 2. Download
    eprintln!("⟳ Starting download...");
    let engine = DownloadEngine::new(client, DownloadConfig::default());

    match engine
        .download(
            url,
            &destination,
            expected_sha256.as_deref(),
            move |progress| {
                print_progress(&progress);
            },
        )
        .await
    {
        Ok(result) => {
            // Clear progress line and print summary
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
