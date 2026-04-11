use anyhow::anyhow;
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi::p2::add_to_linker_sync;

use crate::wast::core::types;

wasmtime::component::bindgen!({
    path: "../../wit",
    world: "syntax-plugin-world",
});

mod file_manager_bindings {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "file-manager-world",
        with: { "wast:core/types": crate::wast::core::types },
    });
}

mod partial_manager_bindings {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "partial-manager-world",
        with: { "wast:core/types": crate::wast::core::types },
    });
}

#[derive(Parser)]
#[command(name = "wast-rs")]
#[command(about = "Rust CLI for loading and invoking WAST wasm components")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate wast.db from world.wit using a file-manager component.
    Bindgen {
        /// Path to file-manager component wasm file.
        #[arg(long = "file-manager")]
        file_manager: String,
        /// Component directory containing world.wit.
        dir: String,
    },
    /// Load a syntax-plugin wasm and run to-text on an empty component.
    ProbeSyntax {
        /// Path to syntax-plugin component wasm file.
        #[arg(long)]
        plugin: String,
    },
    /// Format/validate wast text from stdin using a syntax-plugin component.
    Fmt {
        /// Path to syntax-plugin component wasm file.
        #[arg(long)]
        plugin: String,
    },
    /// Extract functions from a wast.db component as text.
    Extract {
        /// Path to file-manager component wasm file.
        #[arg(long = "file-manager")]
        file_manager: String,
        /// Path to partial-manager component wasm file.
        #[arg(long = "partial-manager")]
        partial_manager: String,
        /// Path to syntax-plugin component wasm file.
        #[arg(long)]
        plugin: String,
        /// Component directory containing wast.db.
        dir: String,
        /// UIDs of functions to extract (uid or uid:include-caller).
        #[arg(required = true)]
        uids: Vec<String>,
    },
    /// Merge text from stdin back into a wast.db component.
    Merge {
        /// Path to file-manager component wasm file.
        #[arg(long = "file-manager")]
        file_manager: String,
        /// Path to syntax-plugin component wasm file.
        #[arg(long)]
        plugin: String,
        /// Component directory containing wast.db.
        dir: String,
        /// Only validate; do not write changes.
        #[arg(long)]
        dry_run: bool,
    },
    /// Compare two wast.db components as text.
    Diff {
        /// Path to file-manager component wasm file.
        #[arg(long = "file-manager")]
        file_manager: String,
        /// Path to syntax-plugin component wasm file.
        #[arg(long)]
        plugin: String,
        /// First component directory containing wast.db.
        dir_a: String,
        /// Second component directory containing wast.db.
        dir_b: String,
    },
    /// Update symbol name mapping for a function or type.
    Syms {
        /// Component directory containing wast.db.
        dir: String,
        /// UID to update.
        uid: String,
        /// Display name.
        name: String,
    },
    /// Configure git diff driver for wast.db files.
    SetupGit,
}

struct HostState {
    table: ResourceTable,
    wasi: WasiCtx,
}

impl HostState {
    fn new(preopen: Option<(&Path, &str)>) -> anyhow::Result<Self> {
        let mut wasi = WasiCtxBuilder::new();
        wasi.inherit_stdio();

        if let Some((host_dir, guest_dir)) = preopen {
            wasi.preopened_dir(host_dir, guest_dir, DirPerms::all(), FilePerms::all())
                .map_err(|e| anyhow!("failed to preopen {} as {guest_dir}: {e}", host_dir.display()))?;
        }

        Ok(Self {
            table: ResourceTable::new(),
            wasi: wasi.build(),
        })
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Bindgen { file_manager, dir } => bindgen_component(&file_manager, &dir),
        Commands::ProbeSyntax { plugin } => probe_syntax(&plugin),
        Commands::Fmt { plugin } => fmt_text(&plugin),
        Commands::Extract { file_manager, partial_manager, plugin, dir, uids } => {
            extract_funcs(&file_manager, &partial_manager, &plugin, &dir, &uids)
        }
        Commands::Merge { file_manager, plugin, dir, dry_run } => {
            merge_text(&file_manager, &plugin, &dir, dry_run)
        }
        Commands::Diff { file_manager, plugin, dir_a, dir_b } => {
            diff_components(&file_manager, &plugin, &dir_a, &dir_b)
        }
        Commands::Syms { dir, uid, name } => {
            update_syms(&dir, &uid, &name)
        }
        Commands::SetupGit => {
            setup_git()
        }
    }
}

fn bindgen_component(file_manager_path: &str, dir: &str) -> anyhow::Result<()> {
    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        return Err(anyhow!("directory does not exist: {dir}"));
    }

    let world_wit = dir_path.join("world.wit");
    if !world_wit.is_file() {
        return Err(anyhow!("world.wit not found: {}", world_wit.display()));
    }

    let db_path = dir_path.join("wast.db");
    if db_path.exists() {
        return Err(anyhow!("wast.db already exists — remove it first to re-generate"));
    }

    let (mut store, fm) = load_file_manager(file_manager_path, dir_path)?;
    let result = fm
        .wast_core_file_manager()
        .call_bindgen(&mut store, "/")
        .map_err(|e| anyhow!("file-manager bindgen failed: {e}"))?;

    if let Err(err) = result {
        return Err(anyhow!("bindgen failed: {}", err.message));
    }

    println!("created {}", db_path.display());
    Ok(())
}

fn probe_syntax(plugin_path: &str) -> anyhow::Result<()> {
    let (mut store, plugin) = load_syntax_plugin(plugin_path)?;

    let empty_component = empty_component();
    let text = plugin
        .wast_core_syntax_plugin()
        .call_to_text(&mut store, &empty_component)
        .map_err(|e| anyhow!("syntax-plugin to-text failed: {e}"))?;

    println!("{}", text);
    Ok(())
}

fn fmt_text(plugin_path: &str) -> anyhow::Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| anyhow!("failed to read stdin: {e}"))?;

    if input.trim().is_empty() {
        return Ok(());
    }

    let (mut store, plugin) = load_syntax_plugin(plugin_path)?;
    let empty_component = empty_component();

    let parsed = plugin
        .wast_core_syntax_plugin()
        .call_from_text(&mut store, &input, &empty_component)
        .map_err(|e| anyhow!("syntax-plugin from-text failed: {e}"))?;

    let component = match parsed {
        Ok(component) => component,
        Err(errors) => {
            let summary = errors
                .into_iter()
                .map(|err| match err.location {
                    Some(location) => format!("{} ({location})", err.message),
                    None => err.message,
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow!("syntax errors: {summary}"));
        }
    };

    let mut formatted = plugin
        .wast_core_syntax_plugin()
        .call_to_text(&mut store, &component)
        .map_err(|e| anyhow!("syntax-plugin to-text failed: {e}"))?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    print!("{}", formatted);
    Ok(())
}

fn load_syntax_plugin(plugin_path: &str) -> anyhow::Result<(Store<HostState>, SyntaxPluginWorld)> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)
        .map_err(|e| anyhow!("failed to create wasmtime engine: {e}"))?;

    let mut linker = Linker::new(&engine);
    add_to_linker_sync(&mut linker)
        .map_err(|e| anyhow!("failed to add WASI to linker: {e}"))?;

    let component = Component::from_file(&engine, plugin_path)
        .map_err(|e| anyhow!("failed to load component from {plugin_path}: {e}"))?;

    let mut store = Store::new(&engine, HostState::new(None)?);

    let plugin = SyntaxPluginWorld::instantiate(&mut store, &component, &linker)
        .map_err(|e| anyhow!("failed to instantiate syntax-plugin world: {e}"))?;

    Ok((store, plugin))
}

fn load_file_manager(
    file_manager_path: &str,
    host_dir: &Path,
) -> anyhow::Result<(Store<HostState>, file_manager_bindings::FileManagerWorld)> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)
        .map_err(|e| anyhow!("failed to create wasmtime engine: {e}"))?;

    let mut linker = Linker::new(&engine);
    add_to_linker_sync(&mut linker)
        .map_err(|e| anyhow!("failed to add WASI to linker: {e}"))?;

    let component = Component::from_file(&engine, file_manager_path)
        .map_err(|e| anyhow!("failed to load component from {file_manager_path}: {e}"))?;

    let mut store = Store::new(&engine, HostState::new(Some((host_dir, "/")))?);

    let file_manager = file_manager_bindings::FileManagerWorld::instantiate(&mut store, &component, &linker)
        .map_err(|e| anyhow!("failed to instantiate file-manager world: {e}"))?;

    Ok((store, file_manager))
}

fn empty_component() -> types::WastComponent {
    types::WastComponent {
        funcs: vec![],
        types: vec![],
        syms: types::Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        },
    }
}

fn extract_funcs(
    file_manager_path: &str,
    partial_manager_path: &str,
    plugin_path: &str,
    dir: &str,
    uid_args: &[String],
) -> anyhow::Result<()> {
    let dir_path = Path::new(dir);

    // Parse uid args: "uid" or "uid:include-caller"
    let targets: Vec<types::ExtractTarget> = uid_args
        .iter()
        .map(|arg| {
            if let Some(uid) = arg.strip_suffix(":include-caller") {
                types::ExtractTarget { sym: uid.to_string(), include_caller: true }
            } else {
                types::ExtractTarget { sym: arg.clone(), include_caller: false }
            }
        })
        .collect();

    // Read full component from file-manager
    let (mut fm_store, fm) = load_file_manager(file_manager_path, dir_path)?;
    let full = fm
        .wast_core_file_manager()
        .call_read(&mut fm_store, "/", Some(&targets))
        .map_err(|e| anyhow!("file-manager read failed: {e}"))?
        .map_err(|e| anyhow!("file-manager read error: {}", e.message))?;

    drop(fm_store);

    // Extract partial component via partial-manager
    let (mut pm_store, pm) = load_partial_manager(partial_manager_path)?;
    let partial = pm
        .wast_core_partial_manager()
        .call_extract(&mut pm_store, &full, &targets)
        .map_err(|e| anyhow!("partial-manager extract failed: {e}"))?;

    drop(pm_store);

    // Render to text via syntax-plugin
    let (mut sp_store, plugin) = load_syntax_plugin(plugin_path)?;
    let text = plugin
        .wast_core_syntax_plugin()
        .call_to_text(&mut sp_store, &partial)
        .map_err(|e| anyhow!("syntax-plugin to-text failed: {e}"))?;

    print!("{}", text);
    Ok(())
}

fn merge_text(
    file_manager_path: &str,
    plugin_path: &str,
    dir: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    let dir_path = Path::new(dir);

    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| anyhow!("failed to read stdin: {e}"))?;

    if input.trim().is_empty() {
        return Err(anyhow!("no input provided"));
    }

    let (mut fm_store, fm) = load_file_manager(file_manager_path, dir_path)?;
    let existing = fm
        .wast_core_file_manager()
        .call_read(&mut fm_store, "/", None)
        .map_err(|e| anyhow!("file-manager read failed: {e}"))?
        .map_err(|e| anyhow!("file-manager read error: {}", e.message))?;

    drop(fm_store);

    // Parse text via syntax-plugin
    let (mut sp_store, plugin) = load_syntax_plugin(plugin_path)?;
    let partial = plugin
        .wast_core_syntax_plugin()
        .call_from_text(&mut sp_store, &input, &existing)
        .map_err(|e| anyhow!("syntax-plugin from-text failed: {e}"))?
        .map_err(|errors| {
            let summary = errors
                .into_iter()
                .map(|e| match e.location {
                    Some(loc) => format!("{} ({loc})", e.message),
                    None => e.message,
                })
                .collect::<Vec<_>>()
                .join("; ");
            anyhow!("syntax errors: {summary}")
        })?;

    drop(sp_store);

    if dry_run {
        println!("dry-run: parse OK, {} funcs", partial.funcs.len());
        return Ok(());
    }

    // Merge into wast.db via file-manager
    let (mut fm_store, fm) = load_file_manager(file_manager_path, dir_path)?;
    fm.wast_core_file_manager()
        .call_merge(&mut fm_store, "/", &partial)
        .map_err(|e| anyhow!("file-manager merge failed: {e}"))?
        .map_err(|e| anyhow!("merge error: {}", e.message))?;

    println!("merged {} funcs into {}/wast.db", partial.funcs.len(), dir);
    Ok(())
}

fn load_partial_manager(
    partial_manager_path: &str,
) -> anyhow::Result<(Store<HostState>, partial_manager_bindings::PartialManagerWorld)> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)
        .map_err(|e| anyhow!("failed to create wasmtime engine: {e}"))?;

    let mut linker = Linker::new(&engine);
    add_to_linker_sync(&mut linker)
        .map_err(|e| anyhow!("failed to add WASI to linker: {e}"))?;

    let component = Component::from_file(&engine, partial_manager_path)
        .map_err(|e| anyhow!("failed to load component from {partial_manager_path}: {e}"))?;

    let mut store = Store::new(&engine, HostState::new(None)?);

    let pm = partial_manager_bindings::PartialManagerWorld::instantiate(&mut store, &component, &linker)
        .map_err(|e| anyhow!("failed to instantiate partial-manager world: {e}"))?;

    Ok((store, pm))
}

fn diff_components(
    file_manager_path: &str,
    plugin_path: &str,
    dir_a: &str,
    dir_b: &str,
) -> anyhow::Result<()> {
    let dir_a_path = Path::new(dir_a);
    let dir_b_path = Path::new(dir_b);

    // Read both components
    let (mut fm_store_a, fm_a) = load_file_manager(file_manager_path, dir_a_path)?;
    let comp_a = fm_a
        .wast_core_file_manager()
        .call_read(&mut fm_store_a, "/", None)
        .map_err(|e| anyhow!("file-manager read (dir_a) failed: {e}"))?
        .map_err(|e| anyhow!("file-manager read error (dir_a): {}", e.message))?;

    drop(fm_store_a);

    let (mut fm_store_b, fm_b) = load_file_manager(file_manager_path, dir_b_path)?;
    let comp_b = fm_b
        .wast_core_file_manager()
        .call_read(&mut fm_store_b, "/", None)
        .map_err(|e| anyhow!("file-manager read (dir_b) failed: {e}"))?
        .map_err(|e| anyhow!("file-manager read error (dir_b): {}", e.message))?;

    drop(fm_store_b);

    // Render both as text
    let (mut sp_store, plugin) = load_syntax_plugin(plugin_path)?;

    let text_a = plugin
        .wast_core_syntax_plugin()
        .call_to_text(&mut sp_store, &comp_a)
        .map_err(|e| anyhow!("syntax-plugin to-text (dir_a) failed: {e}"))?;

    let text_b = plugin
        .wast_core_syntax_plugin()
        .call_to_text(&mut sp_store, &comp_b)
        .map_err(|e| anyhow!("syntax-plugin to-text (dir_b) failed: {e}"))?;

    drop(sp_store);

    if text_a == text_b {
        println!("identical");
        return Ok(());
    }

    println!("=== {} ===", dir_a);
    println!("{}", text_a);
    println!("\n=== {} ===", dir_b);
    println!("{}", text_b);

    Ok(())
}

fn update_syms(dir: &str, uid: &str, display_name: &str) -> anyhow::Result<()> {
    let dir_path = Path::new(dir);
    let syms_file = dir_path.join("syms.en.yaml");

    // Simple syms file format: uid = display_name (one per line, grouped by category)
    let content = if syms_file.exists() {
        fs::read_to_string(&syms_file)
            .map_err(|e| anyhow!("failed to read syms file: {e}"))?
    } else {
        "# Symbol mappings\n".to_string()
    };

    // Determine category (wit, internal, local)
    let category = classify_uid_category(uid);
    let new_content = render_updated_syms_content(&content, uid, display_name);
    fs::write(&syms_file, new_content)
        .map_err(|e| anyhow!("failed to write syms file: {e}"))?;

    println!("{}/{} = \"{}\" -> {}", category, uid, display_name, syms_file.display());
    Ok(())
}

fn setup_git() -> anyhow::Result<()> {
    // Configure git diff driver
    let output = std::process::Command::new("git")
        .args(&["config", "diff.wast.command", "wast diff"])
        .output()
        .map_err(|e| anyhow!("failed to run git config: {e}"))?;

    if !output.status.success() {
        return Err(anyhow!("git config failed — are you inside a git repository?"));
    }

    // Update .gitattributes
    let attribs_path = PathBuf::from(".gitattributes");
    let attrib_line = "wast.db diff=wast";

    let content = if attribs_path.exists() {
        fs::read_to_string(&attribs_path)
            .map_err(|e| anyhow!("failed to read .gitattributes: {e}"))?
    } else {
        String::new()
    };

    if !content.contains(attrib_line) {
        let new_content = ensure_gitattributes_line(&content, attrib_line);
        fs::write(&attribs_path, new_content)
            .map_err(|e| anyhow!("failed to write .gitattributes: {e}"))?;
    }

    println!("git diff driver configured");
    println!(".gitattributes updated: {}", attribs_path.display());
    Ok(())
}

fn classify_uid_category(uid: &str) -> &'static str {
    if uid.len() <= 3 && uid.chars().all(|c| c.is_ascii_lowercase()) {
        "wit"
    } else if uid.starts_with('_') {
        "local"
    } else {
        "internal"
    }
}

fn render_updated_syms_content(content: &str, uid: &str, display_name: &str) -> String {
    let mut found = false;
    let mut updated_lines = Vec::new();

    for line in content.lines() {
        if line.starts_with(&format!("{} = ", uid)) {
            updated_lines.push(format!("{} = \"{}\"", uid, display_name));
            found = true;
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if !found {
        updated_lines.push(format!("{} = \"{}\"", uid, display_name));
    }

    updated_lines.join("\n")
}

fn ensure_gitattributes_line(content: &str, attrib_line: &str) -> String {
    if content.contains(attrib_line) {
        return content.to_string();
    }

    let separator = if content.is_empty() || content.ends_with('\n') { "" } else { "\n" };
    format!("{}{}{}\n", content, separator, attrib_line)
}

#[cfg(test)]
mod tests {
    use super::{classify_uid_category, ensure_gitattributes_line, render_updated_syms_content};

    #[test]
    fn classifies_uid_categories() {
        assert_eq!(classify_uid_category("abc"), "wit");
        assert_eq!(classify_uid_category("_tmp1"), "local");
        assert_eq!(classify_uid_category("func_123"), "internal");
    }

    #[test]
    fn syms_content_updates_existing_uid() {
        let content = "# Symbol mappings\nfunc_1 = \"old\"\nfunc_2 = \"keep\"";
        let updated = render_updated_syms_content(content, "func_1", "new");

        assert!(updated.contains("func_1 = \"new\""));
        assert!(updated.contains("func_2 = \"keep\""));
        assert!(!updated.contains("func_1 = \"old\""));
    }

    #[test]
    fn syms_content_appends_missing_uid() {
        let content = "# Symbol mappings";
        let updated = render_updated_syms_content(content, "func_3", "name");

        assert_eq!(updated, "# Symbol mappings\nfunc_3 = \"name\"");
    }

    #[test]
    fn gitattributes_line_is_added_once() {
        let added = ensure_gitattributes_line("*.txt text\n", "wast.db diff=wast");
        let repeated = ensure_gitattributes_line(&added, "wast.db diff=wast");

        assert_eq!(added, "*.txt text\nwast.db diff=wast\n");
        assert_eq!(repeated, added);
    }
}
