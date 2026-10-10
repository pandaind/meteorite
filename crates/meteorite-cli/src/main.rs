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
}

#[derive(Args, Clone)]
struct BuildFlags {
    /// Include pages marked `draft: true`
    #[arg(long)]
    drafts: bool,
    /// Include pages dated in the future
    #[arg(long)]
    future: bool,
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
    /// Remove an installed theme
    Remove { name: String },
}

fn options(flags: &BuildFlags) -> BuildOptions {
    BuildOptions {
        drafts: flags.drafts,
        future: flags.future,
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

fn run(cli: Cli) -> Result<ExitCode, Error> {
    let root = cli.root;
    match cli.command {
        Command::New {
            what: New::Site { dir, theme },
        } => {
            scaffold::new_site(&dir, &theme)?;
            println!("Created site at {}", dir.display());
            println!("Next:\n  cd {}\n  meteorite serve", dir.display());
        }
        Command::New {
            what: New::Page { path, title },
        } => {
            let file = scaffold::new_page(&root, &path, title.as_deref())?;
            println!("Created {}", file.display());
        }
        Command::Build {
            flags,
            base_url,
            output,
            minify,
        } => {
            let mut opts = options(&flags);
            opts.base_url = base_url;
            opts.out_dir = output;
            opts.minify = minify.then_some(true);
            let report = Builder::new(&root).build(&opts)?;
            print_report(&report);
            println!("Output: {}", report.output.display());
        }
        Command::Serve {
            flags,
            port,
            bind,
            open,
        } => {
            return serve::run(&root, options(&flags), &bind, port, open);
        }
        Command::Check { flags } => {
            let report = check::run(&root, &options(&flags))?;
            for w in &report.build.warnings {
                eprintln!("warning: {w}");
            }
            println!(
                "Checked {} page(s), {} internal link(s)",
                report.files_checked, report.links_checked
            );
            if report.problems.is_empty() {
                println!("No problems found.");
            } else {
                for p in &report.problems {
                    println!("  {}: {} ({})", p.page.display(), p.link, p.reason);
                }
                println!("{} problem(s).", report.problems.len());
                return Ok(ExitCode::from(1));
            }
        }
        Command::Clean => {
            let public = root.join("public");
            if public.is_dir() {
                std::fs::remove_dir_all(&public)
                    .map_err(|e| Error::Msg(format!("{}: {e}", public.display())))?;
                println!("Removed {}", public.display());
            } else {
                println!("Nothing to clean.");
            }
        }
        Command::Theme { what } => theme_cmd(&root, what)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn theme_cmd(root: &Path, what: ThemeCmd) -> Result<(), Error> {
    match what {
        ThemeCmd::List => {
            let active = Config::load(root).map(|c| c.theme).unwrap_or_default();
            let installed = theme::list_installed(root)?;
            if installed.is_empty() {
                println!("No themes installed. Try `meteorite theme add default`.");
            }
            for t in installed {
                let mark = if t.dir_name == active { "*" } else { " " };
                match (t.manifest, t.error) {
                    (Some(m), _) => println!(
                        "{mark} {:<16} {:<8} {}",
                        t.dir_name, m.version, m.description
                    ),
                    (None, Some(e)) => println!("{mark} {:<16} invalid: {e}", t.dir_name),
                    (None, None) => {}
                }
            }
            println!("\nBundled: {}", meteorite_core::bundled::names().join(", "));
        }
        ThemeCmd::Add { source, name } => {
            let installed = theme::add(root, &source, name.as_deref())?;
            println!("Installed theme `{installed}`. Activate it with `theme: {installed}` in config.yml.");
        }
        ThemeCmd::Remove { name } => {
            let active = Config::load(root).map(|c| c.theme).unwrap_or_default();
            theme::remove(root, &name, &active)?;
            println!("Removed theme `{name}`.");
        }
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
