mod chat;
mod serve;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use meteorite_core::build::{BuildOptions, BuildReport, Builder};
use meteorite_core::config::Config;
use meteorite_core::{check, scaffold, theme, Error};

#[derive(Parser)]
#[command(
    name = "meteorite",
    version,
    about = "Markdown + YAML static site generator"
)]
struct Cli {
    /// Site root (defaults to the current directory)
    #[arg(short, long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new site or page
    New {
        #[command(subcommand)]
        what: New,
    },
    /// Build the site into `public/`
    Build {
        #[command(flatten)]
        flags: BuildFlags,
        /// Override `base_url` from config.yml
        #[arg(long)]
        base_url: Option<String>,
        /// Write to this directory instead of `public/`
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Minify HTML and CSS (overrides config)
        #[arg(long)]
        minify: bool,
        /// Skip writing the poptalk-rag knowledge export even if chat.knowledge.dir is set
        #[arg(long)]
        no_export: bool,
    },
    /// Build, serve and live-reload while you edit
    Serve {
        #[command(flatten)]
        flags: BuildFlags,
        #[arg(short, long, default_value_t = 1111)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Open the site in your browser
        #[arg(long)]
        open: bool,
    },
    /// Build to a scratch directory and report broken internal links
    Check {
        #[command(flatten)]
        flags: BuildFlags,
    },
    /// Delete the `public/` directory
    Clean,
    /// Manage themes
    Theme {
        #[command(subcommand)]
        what: ThemeCmd,
    },
    /// Optional PopTalk chat assistant: check the backend, refresh its knowledge base
    Chat {
        #[command(subcommand)]
        what: ChatCmd,
    },
}

#[derive(Args, Clone)]
struct BuildFlags {
    /// Include pages marked `draft: true`
    #[arg(long)]
    drafts: bool,
    /// Include pages dated in the future
    #[arg(long)]
    future: bool,
    /// Use this installed theme instead of the one in config.yml
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,
}

#[derive(Subcommand)]
enum New {
    /// Scaffold a new site in DIR
    Site {
        dir: PathBuf,
        /// Bundled theme to start with
        #[arg(long, default_value = "default")]
        theme: String,
    },
    /// Create content/PATH.md with starter front matter
    Page {
        path: String,
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Subcommand)]
enum ChatCmd {
    /// Verify the PopTalk backend is reachable and allows this site's origin (CORS)
    Check,
    /// Ask poptalk-rag to re-index now (run after `build` has exported the knowledge)
    Sync,
}

#[derive(Subcommand)]
enum ThemeCmd {
    /// List installed themes
    List,
    /// Install a theme from a bundled name, a local directory, or a git URL
    Add {
        source: String,
        /// Install under a different directory name
        #[arg(long)]
        name: Option<String>,
    },
    /// Replace an installed bundled theme with the version in this release (keeps a backup)
    Update {
        /// Theme to update; omit to update every installed bundled theme that is out of date
        name: Option<String>,
    },
    /// Remove an installed theme
    Remove { name: String },
}

fn options(flags: &BuildFlags) -> BuildOptions {
    BuildOptions {
        drafts: flags.drafts,
        future: flags.future,
        theme: flags.theme.clone(),
        ..Default::default()
    }
}

fn print_report(r: &BuildReport) {
    let skipped = r.skipped_drafts + r.skipped_future + r.skipped_expired;
    println!(
        "Built {} page(s) in {} section(s) in {} ms: {} file(s) written, {} unchanged{}{}",
        r.pages,
        r.sections,
        r.elapsed.as_millis(),
        r.files_written,
        r.files_unchanged,
        if r.files_removed > 0 {
            format!(", {} removed", r.files_removed)
        } else {
            String::new()
        },
        if skipped > 0 {
            format!(
                " ({} draft, {} future, {} expired skipped)",
                r.skipped_drafts, r.skipped_future, r.skipped_expired
            )
        } else {
            String::new()
        },
    );
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
}

/// Runs the command line: each subcommand has its own small function below.
fn run(cli: Cli) -> Result<ExitCode, Error> {
    let root = cli.root;
    match cli.command {
        Command::New {
            what: New::Site { dir, theme },
        } => new_site_cmd(&dir, &theme)?,
        Command::New {
            what: New::Page { path, title },
        } => new_page_cmd(&root, &path, title.as_deref())?,
        Command::Build {
            flags,
            base_url,
            output,
            minify,
            no_export,
        } => {
            let mut opts = options(&flags);
            opts.base_url = base_url;
            opts.out_dir = output;
            opts.minify = minify.then_some(true);
            opts.skip_knowledge_export = no_export;
            build_cmd(&root, &opts)?;
        }
        Command::Serve {
            flags,
            port,
            bind,
            open,
        } => return serve::run(&root, options(&flags), &bind, port, open),
        Command::Check { flags } => return check_cmd(&root, &options(&flags)),
        Command::Clean => clean_cmd(&root)?,
        Command::Theme { what } => theme_cmd(&root, what)?,
        Command::Chat { what } => return chat_cmd(&root, what),
    }
    Ok(ExitCode::SUCCESS)
}

fn new_site_cmd(dir: &Path, theme: &str) -> Result<(), Error> {
    scaffold::new_site(dir, theme)?;
    println!("Created site at {}", dir.display());
    println!("Next:\n  cd {}\n  meteorite serve", dir.display());
    Ok(())
}

fn new_page_cmd(root: &Path, path: &str, title: Option<&str>) -> Result<(), Error> {
    let file = scaffold::new_page(root, path, title)?;
    println!("Created {}", file.display());
    Ok(())
}

fn build_cmd(root: &Path, opts: &BuildOptions) -> Result<(), Error> {
    let report = Builder::new(root).build(opts)?;
    print_report(&report);
    println!("Output: {}", report.output.display());
    if let Some(k) = &report.knowledge {
        println!(
            "Knowledge export: {} written, {} unchanged, {} removed in {}",
            k.written,
            k.unchanged,
            k.removed,
            k.dir.display()
        );
    }
    Ok(())
}

/// Builds the site and checks its internal links. Exits with status 1 when there are problems.
fn check_cmd(root: &Path, opts: &BuildOptions) -> Result<ExitCode, Error> {
    let report = check::run(root, opts)?;
    for w in &report.build.warnings {
        eprintln!("warning: {w}");
    }
    println!(
        "Checked {} page(s), {} internal link(s)",
        report.files_checked, report.links_checked
    );
    if report.problems.is_empty() {
        println!("No problems found.");
        return Ok(ExitCode::SUCCESS);
    }
    for p in &report.problems {
        println!("  {}: {} ({})", p.page.display(), p.link, p.reason);
    }
    println!("{} problem(s).", report.problems.len());
    Ok(ExitCode::from(1))
}

fn clean_cmd(root: &Path) -> Result<(), Error> {
    let public = root.join("public");
    if public.is_dir() {
        std::fs::remove_dir_all(&public)
            .map_err(|e| Error::Msg(format!("{}: {e}", public.display())))?;
        println!("Removed {}", public.display());
    } else {
        println!("Nothing to clean.");
    }
    Ok(())
}

fn chat_cmd(root: &Path, what: ChatCmd) -> Result<ExitCode, Error> {
    match what {
        ChatCmd::Check => chat::check(root),
        ChatCmd::Sync => chat::sync(root),
    }
}

fn theme_cmd(root: &Path, what: ThemeCmd) -> Result<(), Error> {
    match what {
        ThemeCmd::List => theme_list(root),
        ThemeCmd::Add { source, name } => {
            let installed = theme::add(root, &source, name.as_deref())?;
            println!("Installed theme `{installed}`. Activate it with `theme: {installed}` in config.yml.");
            Ok(())
        }
        ThemeCmd::Update { name } => theme_update(root, name),
        ThemeCmd::Remove { name } => {
            let active = Config::load(root).map(|c| c.theme).unwrap_or_default();
            theme::remove(root, &name, &active)?;
            println!("Removed theme `{name}`.");
            Ok(())
        }
    }
}

/// Lists the installed themes, marks the active one and points out bundled updates.
fn theme_list(root: &Path) -> Result<(), Error> {
    let active = Config::load(root).map(|c| c.theme).unwrap_or_default();
    let installed = theme::list_installed(root)?;
    if installed.is_empty() {
        println!("No themes installed. Try `meteorite theme add default`.");
    }
    let mut outdated = Vec::new();
    for t in installed {
        let mark = if t.dir_name == active { "*" } else { " " };
        match (t.manifest, t.error) {
            (Some(m), _) => {
                let note = match theme::update_available(&t.dir_name, &m.version) {
                    Some(newer) => {
                        outdated.push(t.dir_name.clone());
                        format!("  [update available: {newer}]")
                    }
                    None => String::new(),
                };
                println!(
                    "{mark} {:<16} {:<8} {}{note}",
                    t.dir_name, m.version, m.description
                )
            }
            (None, Some(e)) => println!("{mark} {:<16} invalid: {e}", t.dir_name),
            (None, None) => {}
        }
    }
    println!("\nBundled: {}", meteorite_core::bundled::names().join(", "));
    if !outdated.is_empty() {
        println!(
            "\nNewer versions of {} ship with this release. Run `meteorite theme update`;\nyour current copy is kept under themes/.backup/.",
            outdated.join(", ")
        );
    }
    Ok(())
}

/// Updates the named theme, or every installed theme that has a newer bundled version.
fn theme_update(root: &Path, name: Option<String>) -> Result<(), Error> {
    let targets: Vec<String> = match name {
        Some(n) => vec![n],
        None => theme::list_installed(root)?
            .into_iter()
            .filter_map(|t| {
                let m = t.manifest?;
                theme::update_available(&t.dir_name, &m.version).map(|_| t.dir_name)
            })
            .collect(),
    };
    if targets.is_empty() {
        println!("All installed bundled themes are up to date.");
    }
    for t in targets {
        let u = theme::update(root, &t)?;
        println!(
            "Updated `{}` {} -> {}. Previous copy: {}",
            u.name,
            u.from,
            u.to,
            u.backup.display()
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            ExitCode::FAILURE
        }
    }
}
