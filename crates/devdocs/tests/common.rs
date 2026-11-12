//! Shared end-to-end CLI test helpers: the isolated per-test environment
//! and the output helper. Each integration test file compiles this module
//! separately (`mod common;`), so items stay public and helpers a file
//! does not use stay.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::must_use_candidate)]
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use devdocs_model::{ResultEnvelope, ResultItem};
use serde_json::Value;

/// Every `DEVDOCS_*` layer variable the host may carry; the tests remove
/// them so only the layers under test apply. `DEVDOCS_HOME` is absent: the
/// env always sets it to the temp data root itself.
const LAYER_VARS: [&str; 10] = [
    "DEVDOCS_FETCH_CONCURRENCY",
    "DEVDOCS_FETCH_TIMEOUT_MS",
    "DEVDOCS_FETCH_MAX_BYTES",
    "DEVDOCS_FETCH_MAX_REDIRECTS",
    "DEVDOCS_FETCH_USER_AGENT",
    "DEVDOCS_INDEX_PATH",
    "DEVDOCS_INDEX_EMBEDDINGS",
    "DEVDOCS_SEARCH_MAX_TOKENS_DEFAULT",
    "DEVDOCS_SEARCH_LIMIT_DEFAULT",
    "DEVDOCS_SEARCH_RRF_K",
];

/// The isolated per-test environment: a temp cwd, data root, and user
/// config dir. Build a fresh [`Command`] per run with [`Env::cmd`].
pub struct Env {
    /// The temp root everything lives under.
    pub root: PathBuf,
    /// The code for the M1 `DEVDOCS_INJECT_ERROR` seam, when set.
    inject: Option<String>,
    /// Extra environment entries applied on top of the isolation.
    extra: Vec<(String, String)>,
    /// Drop `DEVDOCS_HOME` so the `XDG_DATA_HOME` root applies.
    homeless: bool,
}

impl Env {
    /// A fresh isolated environment.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("temp dir").keep(),
            inject: None,
            extra: Vec::new(),
            homeless: false,
        }
    }

    /// Apply the M1 error-injection seam to every run in this env.
    pub fn inject(&mut self, code: &str) {
        self.inject = Some(code.to_owned());
    }

    /// Set one extra environment variable for every run in this env.
    pub fn set(&mut self, var: &str, value: &str) {
        self.extra.push((var.to_owned(), value.to_owned()));
    }

    /// Drop `DEVDOCS_HOME` so the `XDG_DATA_HOME` data root applies.
    pub fn without_home(&mut self) {
        self.homeless = true;
    }

    /// The command with the isolated environment applied.
    pub fn cmd(&self) -> Command {
        Command::from_std(self.std_cmd(&[]))
    }

    /// The raw std command with the isolated environment applied, for
    /// tests that drive the child interactively (`assert_cmd` cannot
    /// hold the pipes open across writes). `args` are the CLI arguments.
    pub fn std_cmd(&self, args: &[&str]) -> std::process::Command {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_devdocs"));
        cmd.args(args);
        cmd.env("XDG_CONFIG_HOME", self.root.join("config"));
        if self.homeless {
            cmd.env_remove("DEVDOCS_HOME")
                .env("XDG_DATA_HOME", self.root.join("data"));
        } else {
            cmd.env("DEVDOCS_HOME", self.root.join("data"));
        }
        for var in LAYER_VARS {
            cmd.env_remove(var);
        }
        match &self.inject {
            Some(code) => {
                cmd.env("DEVDOCS_INJECT_ERROR", code);
            }
            None => {
                cmd.env_remove("DEVDOCS_INJECT_ERROR");
            }
        }
        for (var, value) in &self.extra {
            cmd.env(var, value);
        }
        cmd.current_dir(&self.root);
        cmd
    }
}

/// Run and return (exit code, stdout, stderr).
pub fn output(mut cmd: Command, args: &[&str]) -> (i32, String, String) {
    let out = cmd.args(args).output().expect("binary runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

/// The index and page bodies the acme fixture replay serves.
const INDEX: &str = concat!(
    "# Acme\n\n> Acme widgets documentation.\n\n",
    "## Guides\n\n",
    "- [Intro](https://docs.acme.test/intro.md): Start here\n"
);

const INTRO_V1: &str = concat!(
    "# Intro\n\n",
    "The Acme SDK ships a client for every runtime.\n\n",
    "## Install\n\n",
    "Run the installer and export the key. ",
    "The upgrade guide walks v1 users through the setup.\n"
);

const INTRO_V2: &str = concat!(
    "# Intro\n\n",
    "The Acme SDK v2 ships a widget for every runtime.\n\n",
    "## Install\n\n",
    "Upgrade from v1: run the installer and export the key.\n"
);

/// The replay responses one acme version serves through `--fixture`.
fn acme_responses(intro: &str) -> Value {
    serde_json::json!([
        { "url": "https://docs.acme.test/llms.txt", "status": 200, "headers": {}, "body": INDEX },
        { "url": "https://docs.acme.test/intro.md", "status": 200, "headers": {}, "body": intro },
    ])
}

/// Write one replay manifest: the responses the `--fixture` fetch serves.
fn write_manifest(dir: &Path, responses: &Value) {
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(responses).expect("manifest serializes"),
    )
    .expect("manifest written");
}

/// Write the devdocs.toml pointing at the acme `llms.txt` source under
/// the given `[sources.<name>]` key — the key is the library name; return
/// its path.
fn write_config(root: &Path, name: &str) -> PathBuf {
    let config = root.join("devdocs.toml");
    std::fs::write(
        &config,
        format!(
            concat!(
                "[sources.\"{name}\"]\n",
                "kind = \"llms_txt\"\n",
                "url = \"https://docs.acme.test/llms.txt\"\n",
                "license = \"MIT\"\n",
                "attribution = \"© Acme Corp documentation\"\n"
            ),
            name = name
        ),
    )
    .expect("config written");
    config
}

/// Run one versioned `add --fixture` against the replay manifest.
fn add_fixture_version(env: &Env, dir: &Path, config: &Path, version: &str, responses: &Value) {
    write_manifest(dir, responses);
    let (code, _stdout, stderr) = output(
        env.cmd(),
        &[
            "add",
            "https://docs.acme.test/llms.txt",
            "--fixture",
            dir.to_str().expect("fixture path"),
            "--version",
            version,
            "--json",
            "--config",
            config.to_str().expect("config path"),
        ],
    );
    assert_eq!(code, 0, "add {version} failed: {stderr}");
}

/// An indexed fixture corpus: acme at versions 1.0 and 2.0, at the ingest
/// bootstrap floor (`approved_index`) — no operator promotion. The
/// license-transition tests walk the progression from there.
pub fn indexed_env_at_index() -> (Env, PathBuf) {
    let env = Env::new();
    let dir = env.root.join("fixture");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let config = write_config(&env.root, "acme");
    add_fixture_version(&env, &dir, &config, "1.0", &acme_responses(INTRO_V1));
    add_fixture_version(&env, &dir, &config, "2.0", &acme_responses(INTRO_V2));
    (env, config)
}

/// An indexed fixture corpus: acme at versions 1.0 and 2.0, approved up
/// to the snippet stage. Returns the environment and the config path
/// every CLI run needs.
pub fn indexed_env() -> (Env, PathBuf) {
    let (env, config) = indexed_env_at_index();
    // The ingest bootstrap lands at the publish floor (`approved_index`),
    // which gates verbatim text (docs/09 enforcement point 4); the
    // verbatim-serving tests need the snippet stage. The operator walks
    // the progression one transition at a time, as the CLI would.
    devdocs_testkit::approve_source_to(
        &env.root.join("data").join("docs.db"),
        "acme",
        devdocs_model::LicenseStatus::ApprovedSnippet,
        "test:bootstrap",
    )
    .expect("acme reaches the snippet stage");
    (env, config)
}

/// The intro page carrying a fenced JavaScript definition the symbol pass
/// extracts (docs/implementation/13 §M5): acme 1.0 defines `makeWidget`.
const INTRO_SYMBOL: &str = concat!(
    "# Intro\n\n",
    "The Acme SDK ships a client for every runtime.\n\n",
    "## Widgets\n\n",
    "Create a widget with the factory.\n\n",
    "```js\n",
    "export function makeWidget(name) {\n",
    "  return { name };\n",
    "}\n",
    "```\n"
);

/// An indexed fixture corpus whose page carries an extractable symbol:
/// acme at 1.0 defines `makeWidget`. Returns the environment and the
/// config path every CLI run needs.
pub fn symbol_env() -> (Env, PathBuf) {
    let env = Env::new();
    let dir = env.root.join("fixture");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let config = write_config(&env.root, "acme");
    add_fixture_version(&env, &dir, &config, "1.0", &acme_responses(INTRO_SYMBOL));
    (env, config)
}

/// The symbol corpus of [`symbol_env`] indexed under the scoped library
/// name `@scope/pkg` — the `[sources.<name>]` key is the library name.
/// Returns the environment and the config path every CLI run needs.
pub fn scoped_symbol_env() -> (Env, PathBuf) {
    let env = Env::new();
    let dir = env.root.join("fixture");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let config = write_config(&env.root, "@scope/pkg");
    add_fixture_version(&env, &dir, &config, "1.0", &acme_responses(INTRO_SYMBOL));
    (env, config)
}

/// A second symbol page: `makeWidget` declared again, in its own chunk.
const WIDGETS_SYMBOL: &str = concat!(
    "# Widgets\n\n",
    "The factory grows a second form.\n\n",
    "## Overloads\n\n",
    "```js\n",
    "export function makeWidget(name, zone) {\n",
    "  return { name, zone };\n",
    "}\n",
    "```\n"
);

/// The index listing both symbol pages.
const SYMBOL_INDEX: &str = concat!(
    "# Acme\n\n> Acme widgets documentation.\n\n",
    "## Guides\n\n",
    "- [Intro](https://docs.acme.test/intro.md): Start here\n",
    "- [Widgets](https://docs.acme.test/widgets.md): The widget factory\n"
);

/// The replay responses the two-symbol corpus serves.
fn symbol_responses() -> Value {
    serde_json::json!([
        { "url": "https://docs.acme.test/llms.txt", "status": 200, "headers": {}, "body": SYMBOL_INDEX },
        { "url": "https://docs.acme.test/intro.md", "status": 200, "headers": {}, "body": INTRO_SYMBOL },
        { "url": "https://docs.acme.test/widgets.md", "status": 200, "headers": {}, "body": WIDGETS_SYMBOL },
    ])
}

/// An indexed fixture corpus where the symbol query `makeWidget` matches
/// two chunks (the declaration appears on both pages): the shape the
/// cursor-resume tests need. Returns the environment and the config path.
pub fn two_symbol_env() -> (Env, PathBuf) {
    let env = Env::new();
    let dir = env.root.join("fixture");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let config = write_config(&env.root, "acme");
    add_fixture_version(&env, &dir, &config, "1.0", &symbol_responses());
    (env, config)
}

/// Search the indexed corpus and return the first result: the doc the
/// open/verify surfaces exercise.
pub fn indexed_doc(env: &Env, config: &Path) -> ResultItem {
    let (code, stdout, stderr) = output(
        env.cmd(),
        &[
            "search",
            "client",
            "--json",
            "--config",
            config.to_str().expect("config path"),
        ],
    );
    assert_eq!(code, 0, "search failed: {stderr}");
    let envelope: ResultEnvelope = serde_json::from_str(stdout.trim()).expect("envelope");
    envelope.results.into_iter().next().expect("one result")
}

/// The bytes of one run, with the `served_at` entry cut at the string
/// level — the wall-clock field is the only run-varying value in the
/// envelope. The entry and the comma before it go, so the rest stays
/// valid JSON. The byte-identity tests share this helper so the
/// comparison rule lives once (docs/implementation/08 §CLI contract).
pub fn strip_served_at(raw: &str) -> String {
    let json = raw.trim();
    // The entry is the envelope's last field, so the last occurrence of
    // the key is the top-level one.
    let key = "\"served_at\":\"";
    let at = json.rfind(key).expect("envelope carries served_at");
    let value_starts = at + key.len();
    let value_ends = value_starts + json[value_starts..].find('"').expect("value closes");
    let head = json[..at].strip_suffix(',').unwrap_or(&json[..at]);
    format!("{head}{}", &json[value_ends + 1..])
}

/// ---- The MCP identical-results gate corpus ----
///
/// The source registry the contract corpus registers: the library name,
/// its own replay host (each `add` must match exactly one
/// `[sources.<name>]` entry, so every library carries a distinct URL),
/// and the attribution line. The config writer and the MCP-side config
/// mirror both derive from this one table, so both edges join the same
/// attribution lines.
pub const CONTRACT_SOURCES: [(&str, &str, &str); 5] = [
    ("acme", "docs.acme.test", "© Acme Corp documentation"),
    ("beta", "docs.beta.test", "© Beta Works documentation"),
    (
        "blocked",
        "docs.blocked.test",
        "© Blocked Press documentation",
    ),
    (
        "expiring",
        "docs.expiring.test",
        "© Expiring Press documentation",
    ),
    ("denied", "docs.denied.test", "© Denied Press documentation"),
];

/// The replay host one contract library serves its fixture from.
fn contract_host(name: &str) -> &'static str {
    CONTRACT_SOURCES
        .iter()
        .find(|(library, _, _)| *library == name)
        .map(|(_, host, _)| *host)
        .expect("a registered contract library")
}

/// The same registry as the contract config writes, as the config map
/// the in-process MCP server reads — both sides derive from
/// [`CONTRACT_SOURCES`], so the attribution join cannot drift between
/// the CLI edge and the MCP edge.
pub fn contract_source_configs() -> BTreeMap<String, devdocs_model::config::SourceConfig> {
    CONTRACT_SOURCES
        .iter()
        .map(|(name, host, attribution)| {
            (
                (*name).to_owned(),
                devdocs_model::config::SourceConfig {
                    kind: Some(String::from("llms_txt")),
                    url: Some(format!("https://{host}/llms.txt")),
                    license: Some(String::from("MIT")),
                    attribution: Some((*attribution).to_owned()),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// The contract config: one `[sources.<name>]` entry per
/// [`CONTRACT_SOURCES`] entry, every kind `llms_txt` at its own replay
/// host. Returns the config path every CLI run needs.
fn write_contract_config(root: &Path) -> PathBuf {
    let config = root.join("devdocs.toml");
    let mut text = String::new();
    for (name, host, attribution) in CONTRACT_SOURCES {
        let _ = write!(
            text,
            concat!(
                "[sources.\"{name}\"]\n",
                "kind = \"llms_txt\"\n",
                "url = \"https://{host}/llms.txt\"\n",
                "license = \"MIT\"\n",
                "attribution = \"{attribution}\"\n\n"
            ),
            name = name,
            host = host,
            attribution = attribution
        );
    }
    std::fs::write(&config, text).expect("config written");
    config
}

/// The acme contract pages at 1.0: the page slug (the ref segment) and
/// the page body. Every plain query hits one of these bodies verbatim.
const ACME_V1_PAGES: [(&str, &str); 5] = [
    (
        "getting-started",
        concat!(
            "# Getting Started\n\n",
            "The quick start builds the first widget project step by step.\n\n",
            "## Prerequisites\n\n",
            "Install the toolchain and set the API key before the first build.\n"
        ),
    ),
    (
        "widgets",
        concat!(
            "# Widgets\n\n",
            "A widget renders a boxed frame on the screen.\n\n",
            "## Layout\n\n",
            "The layout engine stacks widgets in a column.\n"
        ),
    ),
    (
        "install",
        concat!(
            "# Install\n\n",
            "Run the installer and export the key.\n\n",
            "## Upgrade\n\n",
            "The upgrade guide walks v1 users through the setup.\n"
        ),
    ),
    (
        "reference",
        concat!(
            "# Reference\n\n",
            "The API reference lists every method and parameter.\n\n",
            "## Errors\n\n",
            "Every method reports errors through the error channel.\n"
        ),
    ),
    (
        "symbols",
        concat!(
            "# Symbols\n\n",
            "The factory functions live in the core module.\n\n",
            "## makeWidget\n\n",
            "Create a widget with the factory.\n\n",
            "```js\n",
            "export function makeWidget(name) {\n",
            "  return { name };\n",
            "}\n",
            "```\n\n",
            "## renderBox\n\n",
            "Paint one boxed frame.\n\n",
            "```js\n",
            "export function renderBox(node) {\n",
            "  return node;\n",
            "}\n",
            "```\n"
        ),
    ),
];

/// The acme contract pages at 2.0: the same shape with distinct wording,
/// so version-pinned queries separate the versions.
const ACME_V2_PAGES: [(&str, &str); 5] = [
    (
        "getting-started",
        concat!(
            "# Getting Started\n\n",
            "The quick start builds the first dashboard project step by step.\n\n",
            "## Prerequisites\n\n",
            "Install the toolchain and set the access token before the first build.\n"
        ),
    ),
    (
        "widgets",
        concat!(
            "# Widgets\n\n",
            "A widget paints a panel on the dashboard.\n\n",
            "## Layout\n\n",
            "The layout engine arranges widgets in a grid.\n"
        ),
    ),
    (
        "install",
        concat!(
            "# Install\n\n",
            "Upgrade from v1: run the installer and import the key.\n\n",
            "## Migration\n\n",
            "The migration guide walks v1 users through the new setup.\n"
        ),
    ),
    (
        "reference",
        concat!(
            "# Reference\n\n",
            "The API reference documents every method and option.\n\n",
            "## Errors\n\n",
            "Every method reports failures through the failure channel.\n"
        ),
    ),
    (
        "symbols",
        concat!(
            "# Symbols\n\n",
            "The factory functions live in the core module.\n\n",
            "## makeWidget\n\n",
            "Create a widget with the zone-aware factory.\n\n",
            "```js\n",
            "export function makeWidget(name, zone) {\n",
            "  return { name, zone };\n",
            "}\n",
            "```\n\n",
            "## paintPanel\n\n",
            "Paint one dashboard panel.\n\n",
            "```js\n",
            "export function paintPanel(node) {\n",
            "  return node;\n",
            "}\n",
            "```\n"
        ),
    ),
];

/// The beta pages: the second library the library-scoped queries scope to.
const BETA_PAGES: [(&str, &str); 2] = [
    (
        "starter",
        concat!(
            "# Starter\n\n",
            "The beta starter configures the sandbox and connects the client.\n"
        ),
    ),
    (
        "notes",
        concat!(
            "# Notes\n\n",
            "The beta notes list the sandbox fixes for the release.\n"
        ),
    ),
];

/// The blocked page: ingested and active, then revoked before any query.
const BLOCKED_PAGES: [(&str, &str); 1] = [(
    "blocked",
    "# Blocked\n\nThe blocked marker page never serves its text.\n",
)];

/// The expiring page: ingested and active, then given a past expiry so
/// the read-path sweep flips it at the first query.
const EXPIRING_PAGES: [(&str, &str); 1] = [(
    "expiring",
    "# Expiring\n\nThe expiring marker page leaves at the sweep.\n",
)];

/// The denied page: ingested and active, then denied before any query.
const DENIED_PAGES: [(&str, &str); 1] = [(
    "denied",
    "# Denied\n\nThe denied marker page never serves its text.\n",
)];

/// The replay responses one multi-page corpus serves: the `llms.txt`
/// index listing every page, then one response per page, all at the
/// library's own replay host.
fn pages_responses(host: &str, pages: &[(&str, &str)]) -> Value {
    let mut index = String::from("# Corpus\n\n> The contract corpus.\n\n## Guides\n\n");
    let mut responses = Vec::with_capacity(pages.len() + 1);
    for (slug, body) in pages {
        let _ = writeln!(
            index,
            "- [{slug}](https://{host}/{slug}.md): The {slug} page"
        );
        responses.push(serde_json::json!({
            "url": format!("https://{host}/{slug}.md"),
            "status": 200,
            "headers": {},
            "body": body,
        }));
    }
    responses.insert(
        0,
        serde_json::json!({
            "url": format!("https://{host}/llms.txt"),
            "status": 200,
            "headers": {},
            "body": index,
        }),
    );
    Value::Array(responses)
}

/// Runs one versioned `add --fixture` for one contract library, using
/// its own manifest directory so replays never clobber each other.
fn add_contract_pages(
    env: &Env,
    dir: &Path,
    config: &Path,
    name: &str,
    version: &str,
    pages: &[(&str, &str)],
) {
    let host = contract_host(name);
    let manifest = dir.join(format!("{name}-{version}"));
    std::fs::create_dir_all(&manifest).expect("manifest dir");
    write_manifest(&manifest, &pages_responses(host, pages));
    let (code, _stdout, stderr) = output(
        env.cmd(),
        &[
            "add",
            &format!("https://{host}/llms.txt"),
            "--fixture",
            manifest.to_str().expect("fixture path"),
            "--version",
            version,
            "--json",
            "--config",
            config.to_str().expect("config path"),
        ],
    );
    assert_eq!(code, 0, "add {name} {version} failed: {stderr}");
}

/// The indexed contract corpus for the MCP identical-results gate:
/// `acme` at 1.0 and 2.0 (five pages each) and `beta` at 1.0, all
/// approved to the snippet stage, plus the license-refused trio
/// (`blocked` revoked, `expiring` past expiry, `denied` denied) seeded
/// active so the direct-name queries reach the license gate. Returns the
/// environment and the config path every CLI run needs.
pub fn contract_env() -> (Env, PathBuf) {
    let env = Env::new();
    let dir = env.root.join("fixture");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let config = write_contract_config(&env.root);
    add_contract_pages(&env, &dir, &config, "acme", "1.0", &ACME_V1_PAGES);
    add_contract_pages(&env, &dir, &config, "acme", "2.0", &ACME_V2_PAGES);
    add_contract_pages(&env, &dir, &config, "beta", "1.0", &BETA_PAGES);
    add_contract_pages(&env, &dir, &config, "blocked", "1.0", &BLOCKED_PAGES);
    add_contract_pages(&env, &dir, &config, "expiring", "1.0", &EXPIRING_PAGES);
    add_contract_pages(&env, &dir, &config, "denied", "1.0", &DENIED_PAGES);

    let db = env.root.join("data").join("docs.db");
    // acme and beta serve verbatim text: the operator promotes both to
    // the snippet stage, the same walk `indexed_env` takes. The refused
    // trio keeps its ingest floor — the license gate refuses before any
    // text serves.
    for library in ["acme", "beta"] {
        devdocs_testkit::approve_source_to(
            &db,
            library,
            devdocs_model::LicenseStatus::ApprovedSnippet,
            "test:bootstrap",
        )
        .expect("approval lands");
    }
    devdocs_testkit::transition_source(
        &db,
        "blocked",
        devdocs_model::LicenseStatus::Revoked,
        "test:revoke",
    )
    .expect("revocation lands");
    devdocs_testkit::transition_source(
        &db,
        "denied",
        devdocs_model::LicenseStatus::Denied,
        "test:deny",
    )
    .expect("denial lands");
    devdocs_testkit::set_source_expiry(&db, "expiring", "2026-01-01").expect("expiry planted");
    (env, config)
}
