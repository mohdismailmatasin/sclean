mod cleaner;
mod config;
mod error;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::io;
use std::path::PathBuf;
use std::process;
use std::time::Instant;
use tracing::info;
use tracing_subscriber::EnvFilter;

use cleaner::{
    clean_directory, clean_journal_logs, clean_orphans, clean_pacman_download_cache,
    clean_pacman_old_packages, clean_system_logs, docker_prune, format_size, remove_empty_dirs,
    remove_file, remove_lock_file, CleanResult,
};
use config::{Config, TargetType};

#[derive(Parser, Debug)]
#[command(name = "sclean")]
#[command(version = "0.4.0")]
#[command(about = "A lightweight system cleanup utility for Linux")]
#[command(after_help = "EXAMPLES:
  sclean                    # Clean enabled targets
  sclean --preview         # Preview what would be cleaned
  sclean --targets cache   # Clean only cache directories
  sclean -t temp,cache    # Clean temp and cache
  sclean --json           # Output results as JSON
  sclean --report out.json # Export results to JSON file
  sclean --verbose       # Show detailed progress

  # Generate shell completions
  sclean --completions bash > /etc/bash_completion.d/sclean
  sclean --completions zsh > ~/.zsh/completions/_sclean
  sclean --completions fish > ~/.config/fish/completions/sclean.fish
")]
struct Cli {
    #[arg(short, long, help = "Preview what would be cleaned without deleting")]
    preview: bool,

    #[arg(short, long, help = "Same as --preview, show files without deleting")]
    dry_run: bool,

    #[arg(short, long, help = "Enable verbose output with detailed information")]
    verbose: bool,

    #[arg(short, long, help = "Suppress most output")]
    quiet: bool,

    #[arg(
        short,
        long,
        help = "Interactive mode - prompt before each cleanup category"
    )]
    interactive: bool,

    #[arg(
        short = 't',
        long,
        help = "Comma-separated list of targets to clean (e.g., temp,cache,browser)"
    )]
    targets: Option<String>,

    #[arg(long, help = "List all available clean targets")]
    list_targets: bool,

    #[arg(long, help = "Generate default config file and exit")]
    generate_config: bool,

    #[arg(long, help = "Generate shell completions for the specified shell")]
    completions: Option<Shell>,

    #[arg(long, help = "Output results as JSON", short = 'j')]
    json: bool,

    #[arg(long, help = "Export results to a file (JSON or CSV format)")]
    report: Option<String>,
}

struct Task {
    name: String,
    path: PathBuf,
    target_type: TargetType,
    enabled: bool,
}

fn print_banner(dry_run: bool, is_root: bool) {
    let mode = if dry_run { "preview" } else { "cleanup" };

    println!(
        "  {}",
        r#"
  /$$$$$$   /$$$$$$  /$$       /$$$$$$$$  /$$$$$$  /$$   /$$
 /$$__  $$ /$$__  $$| $$      | $$_____/ /$$__  $$| $$$ | $$
| $$  \__/| $$  \__/| $$      | $$      | $$  \ $$| $$$$| $$
|  $$$$$$ | $$      | $$      | $$$$$   | $$$$$$$$| $$ $$ $$
 \____  $$| $$      | $$      | $$__/   | $$__  $$| $$  $$$$
 /$$  \ $$| $$    $$| $$      | $$      | $$  | $$| $$\  $$$
|  $$$$$$/|  $$$$$$/| $$$$$$$$| $$$$$$$$| $$  | $$| $$ \  $$
 \______/  \______/ |________/|________/|__/  |__/|__/  \__/
"#
        .truecolor(0, 255, 200)
        .bold()
    );

    println!(
        "  {} {}  {}",
        "sclean".cyan().bold(),
        "v0.4.0".dimmed(),
        format!("[{}]", mode).dimmed()
    );

    if !is_root {
        println!(
            "  {}",
            "warning: running as non-root, some cleanups may be skipped"
                .yellow()
                .dimmed()
        );
    }

    println!();
}

fn create_progress_bar(len: usize) -> ProgressBar {
    let pb = ProgressBar::new(len as u64);
    pb.set_style(
        ProgressStyle::with_template("  {prefix:.bold.dim} {spinner:.cyan} {wide_msg} {pos}/{len}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn read_yes_no(prompt: &str) -> bool {
    use std::io::Write;
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = write!(handle, "  {} ", prompt.yellow().bold());
    let _ = handle.flush();

    let mut s = String::new();
    let _ = io::stdin().read_line(&mut s);
    let ch = s.chars().next().unwrap_or('n');
    ch == 'y' || ch == 'Y'
}

fn prompt_confirm(pb: Option<&ProgressBar>, prompt: &str) -> bool {
    if let Some(pb) = pb {
        pb.suspend(|| read_yes_no(prompt))
    } else {
        read_yes_no(prompt)
    }
}

fn print_report_table(report: &[(String, String, CleanResult)], dry_run: bool) {
    if report.is_empty() {
        println!("  {}\n", "nothing to clean".green());
        return;
    }

    let total_size: u64 = report.iter().map(|(_, _, r)| r.size).sum();
    let total_files: u64 = report.iter().map(|(_, _, r)| r.files_removed).sum();
    let total_dirs: u64 = report.iter().map(|(_, _, r)| r.dirs_removed).sum();

    let sep = "  ────────────────────────────────────────────────".dimmed();
    println!("\n{sep}\n  {}\n{sep}", "Results".bold());

    for (i, (desc, _path, result)) in report.iter().enumerate() {
        let details = build_result_details(result);
        let label = if details.is_empty() {
            desc.dimmed().to_string()
        } else {
            format!("{} ({})", desc, details.dimmed())
        };

        println!(
            "  {} {}  {}",
            format!("{}.", i + 1).dimmed(),
            format_size(result.size).green().bold(),
            label
        );
    }

    println!("{sep}");
    println!(
        "  {}",
        [
            format_size(total_size).green().bold().to_string(),
            format!("{} files", total_files.to_string().cyan()),
            format!("{} dirs", total_dirs.to_string().cyan()),
        ]
        .join("  ")
    );
    println!();

    if dry_run {
        println!(
            "  {} {}",
            "→".blue(),
            "run without --preview to actually clean".blue().dimmed()
        );
    } else {
        println!("  {}", "done".green().bold());
    }
    println!();
}

#[derive(Debug, Serialize)]
struct ReportEntry {
    name: String,
    path: String,
    size: u64,
    size_formatted: String,
    files_removed: u64,
    dirs_removed: u64,
    cleaned: bool,
}

impl From<(String, String, CleanResult)> for ReportEntry {
    fn from((name, path, result): (String, String, CleanResult)) -> Self {
        let cleaned = result.size > 0 || result.files_removed > 0 || result.dirs_removed > 0;
        Self {
            name,
            path,
            size: result.size,
            size_formatted: format_size(result.size),
            files_removed: result.files_removed,
            dirs_removed: result.dirs_removed,
            cleaned,
        }
    }
}

fn export_json_report(
    entries: &[ReportEntry],
    total_size: u64,
    total_files: u64,
    total_dirs: u64,
    duration: std::time::Duration,
    dry_run: bool,
) -> String {
    #[derive(Serialize)]
    struct JsonReport<'a> {
        version: &'a str,
        dry_run: bool,
        total_size: u64,
        total_size_formatted: String,
        total_files_removed: u64,
        total_dirs_removed: u64,
        duration_seconds: f64,
        entries: &'a [ReportEntry],
    }

    let report = JsonReport {
        version: "0.4.0",
        dry_run,
        total_size,
        total_size_formatted: format_size(total_size),
        total_files_removed: total_files,
        total_dirs_removed: total_dirs,
        duration_seconds: duration.as_secs_f64(),
        entries,
    };

    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
}

fn export_csv_report(entries: &[ReportEntry]) -> String {
    let mut csv =
        String::from("name,path,size,size_formatted,files_removed,dirs_removed,cleaned\n");
    for entry in entries {
        csv.push_str(&format!(
            "\"{}\",\"{}\",{},{},{},{},{}\n",
            entry.name.replace('"', "\"\""),
            entry.path.replace('"', "\"\""),
            entry.size,
            entry.size_formatted,
            entry.files_removed,
            entry.dirs_removed,
            entry.cleaned
        ));
    }
    csv
}

fn build_result_details(result: &CleanResult) -> String {
    let mut parts = Vec::new();
    if result.files_removed > 0 {
        parts.push(format!("{} files", result.files_removed));
    }
    if result.dirs_removed > 0 {
        parts.push(format!("{} dirs", result.dirs_removed));
    }
    parts.join(", ")
}

fn print_targets_list(tasks: &[Task]) {
    let sep = "  ────────────────────────────────────────────────".dimmed();
    println!("\n{sep}\n  {}\n{sep}", "Clean Targets".bold());

    for (i, task) in tasks.iter().enumerate() {
        let status = if task.enabled {
            "enabled".green()
        } else {
            "disabled".yellow().dimmed()
        };
        let icon = if task.enabled { "●" } else { "○" };
        println!(
            "  {} {:<2}. {:<30} {}",
            icon,
            format!("{}", i + 1).dimmed(),
            task.name,
            status
        );
        println!("     {}", task.path.display().to_string().dimmed());
    }
    println!();
}

fn print_summary_dashboard(
    total_cleaned: u64,
    total_tasks: usize,
    tasks_with_results: usize,
    duration: std::time::Duration,
) {
    println!(
        "  {}",
        [
            format!("{} tasks", total_tasks).dimmed().to_string(),
            format!("{} cleaned", tasks_with_results)
                .dimmed()
                .to_string(),
            format_size(total_cleaned).green().bold().to_string(),
            format_duration(duration).dimmed().to_string(),
        ]
        .join("  ")
    );
    println!();
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn init_logging(verbose: bool, quiet: bool) {
    if quiet || !verbose {
        return;
    }

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sclean=debug"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .without_time()
        .with_writer(io::stderr)
        .try_init();
}

fn main() {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet);

    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        generate(shell, &mut cmd, "sclean", &mut io::stdout());
        return;
    }

    let config = Config::load();

    if cli.generate_config {
        handle_generate_config(&config);
        return;
    }

    if cli.list_targets {
        print_targets_list(&build_tasks(&config));
        return;
    }

    let dry_run = cli.preview || cli.dry_run;
    let root = is_root();

    if !cli.quiet {
        print_banner(dry_run, root);
    }

    let tasks = build_tasks(&config);
    let selected_tasks = select_tasks(&tasks, &cli);

    if selected_tasks.is_empty() {
        eprintln!(
            "\n  {}\n",
            "no clean targets selected, use --list-targets to see available targets".red()
        );
        process::exit(1);
    }

    if cli.interactive && dry_run {
        eprintln!(
            "\n  {}\n",
            "interactive mode is not available with --preview or --dry-run".yellow()
        );
        process::exit(1);
    }

    let (total_cleaned, report, duration) = run_tasks(&selected_tasks, dry_run, &config, &cli);

    let entries: Vec<ReportEntry> = report
        .iter()
        .map(|(name, path, result)| ReportEntry::from((name.clone(), path.clone(), result.clone())))
        .collect();

    if cli.json {
        let output = export_json_report(
            &entries,
            total_cleaned,
            report.iter().map(|(_, _, r)| r.files_removed).sum(),
            report.iter().map(|(_, _, r)| r.dirs_removed).sum(),
            duration,
            dry_run,
        );
        println!("{}", output);
    } else if let Some(ref report_path) = cli.report {
        if report_path.ends_with(".csv") || report_path.ends_with(".CSV") {
            let output = export_csv_report(&entries);
            if let Err(e) = std::fs::write(report_path, output) {
                eprintln!("  {} Failed to write report: {}", "error".red(), e);
                process::exit(1);
            }
            if !cli.quiet {
                println!(
                    "  {} report exported to {}",
                    "✓".green(),
                    report_path.dimmed()
                );
            }
        } else {
            let output = export_json_report(
                &entries,
                total_cleaned,
                report.iter().map(|(_, _, r)| r.files_removed).sum(),
                report.iter().map(|(_, _, r)| r.dirs_removed).sum(),
                duration,
                dry_run,
            );
            if let Err(e) = std::fs::write(report_path, output) {
                eprintln!("  {} Failed to write report: {}", "error".red(), e);
                process::exit(1);
            }
            if !cli.quiet {
                println!(
                    "  {} report exported to {}",
                    "✓".green(),
                    report_path.dimmed()
                );
            }
        }
    } else if !cli.quiet {
        println!();
        print_summary_dashboard(total_cleaned, selected_tasks.len(), report.len(), duration);
        print_report_table(&report, dry_run);
    }

    info!("Simple Cleaner finished");
}

fn handle_generate_config(_config: &Config) {
    let default_config = Config::generate_default();
    match default_config.save() {
        Ok(()) => {
            println!(
                "\n  {} {}\n",
                "✓".green(),
                format!("config generated at: {}", Config::config_path().display()).dimmed()
            );
        }
        Err(e) => {
            eprintln!(
                "\n  {}\n",
                format!("failed to generate config: {}", e).red()
            );
            process::exit(1);
        }
    }
}

fn select_tasks<'a>(tasks: &'a [Task], cli: &'a Cli) -> Vec<&'a Task> {
    if let Some(ref target_list) = cli.targets {
        let selected: Vec<String> = target_list
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();
        tasks
            .iter()
            .filter(|t| {
                selected.iter().any(|s| {
                    let name_lower = t.name.to_lowercase();
                    name_lower.contains(s) || matches_target_alias(&t.name, s)
                })
            })
            .collect()
    } else {
        tasks.iter().filter(|t| t.enabled).collect()
    }
}

fn run_tasks(
    selected_tasks: &[&Task],
    dry_run: bool,
    config: &Config,
    cli: &Cli,
) -> (u64, Vec<(String, String, CleanResult)>, std::time::Duration) {
    let total_tasks = selected_tasks.len();
    let mut total_cleaned: u64 = 0;
    let mut report = Vec::new();
    let start_time = Instant::now();

    let pb = if !cli.quiet {
        Some(create_progress_bar(total_tasks))
    } else {
        None
    };

    for (i, task) in selected_tasks.iter().enumerate() {
        if let Some(ref pb) = pb {
            pb.set_prefix(format!("[{}/{}]", i + 1, total_tasks));
            pb.set_message(task.name.clone());
        }

        if !should_skip_task(task, &pb, cli) {
            let result = execute_task(task, dry_run, config, cli.verbose);

            if result.size > 0 || result.files_removed > 0 || result.dirs_removed > 0 {
                total_cleaned += result.size;
                report.push((
                    task.name.clone(),
                    task.path.display().to_string(),
                    result.clone(),
                ));
            }
        }

        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }

    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let elapsed = start_time.elapsed();

    info!(
        elapsed = ?elapsed,
        cleaned = total_cleaned,
        tasks = report.len(),
        "cleanup completed"
    );

    (total_cleaned, report, elapsed)
}

fn should_skip_task(task: &Task, pb: &Option<ProgressBar>, cli: &Cli) -> bool {
    if task.name == "User trash"
        && !prompt_confirm(pb.as_ref().map(|p| p as &ProgressBar), "clear Trash? [y/N]")
    {
        return true;
    }

    if cli.interactive
        && !prompt_confirm(
            pb.as_ref().map(|p| p as &ProgressBar),
            &format!("clean \"{}\"? [y/N]", task.name),
        )
    {
        return true;
    }

    false
}

fn matches_target_alias(name: &str, alias: &str) -> bool {
    let lower = name.to_lowercase();
    match alias {
        "temp" => lower.contains("temp"),
        "cache" => lower.contains("cache"),
        "browser" => {
            lower.contains("firefox") || lower.contains("chrome") || lower.contains("chromium")
        }
        "dev" => {
            lower.contains("npm")
                || lower.contains("yarn")
                || lower.contains("bun")
                || lower.contains("pnpm")
                || lower.contains("pip")
                || lower.contains("poetry")
                || lower.contains("cargo")
                || lower.contains("go-build")
                || lower.contains("composer")
        }
        "python" => lower.contains("pip") || lower.contains("poetry") || lower.contains("uv"),
        "system" => {
            lower.contains("apt")
                || lower.contains("pacman")
                || lower.contains("flatpak")
                || lower.contains("snap")
        }
        "logs" => lower.contains("log"),
        "trash" => lower.contains("trash"),
        "empty" => lower.contains("empty"),
        "orphan" => lower.contains("orphan"),
        "lock" => lower.contains("lock"),
        "docker" => lower.contains("docker"),
        "electron" => {
            lower.contains("discord") || lower.contains("slack") || lower.contains("teams")
        }
        "gpu" => lower.contains("mesa") || lower.contains("nvidia") || lower.contains("shader"),
        _ => false,
    }
}

fn build_tasks(config: &Config) -> Vec<Task> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home"));
    let cache = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/home/.cache"));

    if !config.targets.is_empty() {
        return config
            .targets
            .iter()
            .map(|t| Task {
                name: t.name.clone(),
                path: PathBuf::from(&t.path),
                target_type: t.target_type.clone(),
                enabled: t.enabled,
            })
            .collect();
    }

    macro_rules! dir {
        ($name:expr, $path:expr) => {
            Task {
                name: $name.into(),
                path: $path,
                target_type: TargetType::Directory,
                enabled: true,
            }
        };
    }

    macro_rules! dir_home {
        ($name:expr, $path:expr) => {
            dir!($name, home.join($path))
        };
    }

    vec![
        dir!("User temp files", PathBuf::from("/tmp")),
        dir!("System temp files", PathBuf::from("/var/tmp")),
        dir!("User cache", cache.clone()),
        dir_home!("Thumbnail cache", ".cache/thumbnails"),
        dir_home!("Firefox cache", ".cache/mozilla/firefox"),
        dir_home!("Chrome cache", ".config/google-chrome/Default/Cache"),
        dir_home!("Chromium cache", ".cache/chromium"),
        dir_home!("Python pip cache", ".cache/pip"),
        dir_home!("Node.js npm cache", ".npm"),
        dir_home!("Yarn cache", ".cache/yarn"),
        dir_home!("Bun cache", ".cache/bun"),
        dir_home!("pnpm cache", ".local/share/pnpm"),
        dir_home!("Poetry cache", ".cache/pypoetry"),
        dir_home!("uv cache", ".cache/uv"),
        dir_home!("Rust cargo cache", ".cargo/registry"),
        dir_home!("Flatpak app cache", ".cache/flatpak"),
        dir_home!("PHP composer cache", ".composer/cache"),
        dir_home!("Go build cache", ".cache/go-build"),
        dir!("APT package cache", PathBuf::from("/var/cache/apt")),
        dir!("Pacman package cache", PathBuf::from("/var/cache/pacman")),
        Task {
            name: "Pacman old packages".into(),
            path: PathBuf::from("/var/cache/pacman/pkg"),
            target_type: TargetType::PacmanOldPkgs,
            enabled: true,
        },
        Task {
            name: "Pacman download cache".into(),
            path: PathBuf::from("/var/cache/pacman/pkg"),
            target_type: TargetType::PacmanDownloadCache,
            enabled: true,
        },
        dir!("Snap cache", PathBuf::from("/var/lib/snapd/cache")),
        dir!("Flatpak system cache", PathBuf::from("/var/cache/flatpak")),
        dir_home!("Discord cache", ".config/Discord/Cache"),
        dir_home!("Slack cache", ".config/Slack/Cache"),
        dir_home!("VS Code cache", ".config/Code/Cache"),
        dir_home!("VS Code cached data", ".config/Code/CachedData"),
        dir_home!("JetBrains IDE caches", ".cache/JetBrains"),
        dir_home!("Spotify cache", ".cache/spotify"),
        dir_home!("Steam shader cache", ".local/share/Steam/shadercache"),
        dir_home!("Steam temp files", ".local/share/Steam/temp"),
        dir_home!("Mesa shader cache", ".cache/mesa_shader_cache"),
        dir_home!("NVIDIA GL cache", ".cache/nvidia/GLCache"),
        dir_home!("Font cache", ".cache/fontconfig"),
        dir_home!("Tracker indexer cache (v3)", ".cache/tracker3"),
        dir_home!("Tracker indexer cache (v2)", ".cache/tracker"),
        dir_home!("Baloo file indexer cache", ".local/share/baloo"),
        Task {
            name: "Recent files".into(),
            path: home.join(".local/share/recently-used.xbel"),
            target_type: TargetType::File,
            enabled: true,
        },
        dir!("System logs", PathBuf::from("/var/log")),
        dir!("System journal logs", PathBuf::from("/var/log/journal")),
        dir!("Core dumps", PathBuf::from("/var/lib/systemd/coredump")),
        dir_home!("User trash", ".local/share/Trash"),
        Task {
            name: "Docker prune".into(),
            path: PathBuf::from("/var/lib/docker"),
            target_type: TargetType::DockerPrune,
            enabled: false,
        },
        Task {
            name: "Empty directories in home".into(),
            path: home.clone(),
            target_type: TargetType::EmptyDirs,
            enabled: false,
        },
        Task {
            name: "Orphan packages (Arch)".into(),
            path: PathBuf::from("/usr"),
            target_type: TargetType::Orphans,
            enabled: true,
        },
        Task {
            name: "Pacman lock file".into(),
            path: PathBuf::from("/var/lib/pacman/db.lck"),
            target_type: TargetType::LockFile,
            enabled: true,
        },
    ]
}

fn execute_task(task: &Task, dry_run: bool, config: &Config, verbose: bool) -> CleanResult {
    match &task.target_type {
        TargetType::Directory => {
            let max_age = task_max_age(&task.name, config);

            if task.name == "System journal logs" {
                clean_journal_logs(dry_run, &config.protected_dirs, verbose)
            } else if task.name == "System logs" {
                clean_system_logs(
                    &task.path,
                    dry_run,
                    config.max_log_age_days,
                    &config.protected_dirs,
                    verbose,
                )
            } else {
                clean_directory(
                    &task.path,
                    dry_run,
                    &config.protected_dirs,
                    max_age,
                    verbose,
                )
            }
        }
        TargetType::EmptyDirs => {
            remove_empty_dirs(&task.path, dry_run, &config.protected_dirs, verbose)
        }
        TargetType::Orphans => clean_orphans(dry_run, verbose),
        TargetType::LockFile => remove_lock_file(&task.path, dry_run, verbose),
        TargetType::DockerPrune => docker_prune(dry_run, verbose),
        TargetType::PacmanOldPkgs => clean_pacman_old_packages(2, dry_run, verbose),
        TargetType::PacmanDownloadCache => clean_pacman_download_cache(dry_run, verbose),
        TargetType::File => remove_file(&task.path, dry_run, verbose),
    }
}

fn task_max_age(task_name: &str, config: &Config) -> Option<u64> {
    let lower = task_name.to_lowercase();
    if lower.contains("log") {
        Some(config.max_log_age_days)
    } else if lower.contains("temp") {
        Some(config.max_temp_age_days)
    } else {
        None
    }
}
