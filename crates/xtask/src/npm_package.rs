//! npm package metadata generation and release staging.

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Args};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("mode")
        .required(true)
        .args(["check", "generate", "stage"])
))]
pub struct NpmArgs {
    /// Check tracked metadata against canonical generated values.
    #[arg(long)]
    pub check: bool,
    /// Write canonical metadata and licenses into the repository.
    #[arg(long)]
    pub generate: bool,
    /// Stage all packages into a new, separate output directory.
    #[arg(long, requires = "binaries")]
    pub stage: Option<PathBuf>,
    /// Directory containing unpacked target archives, keyed by Rust target.
    #[arg(long, requires = "stage")]
    pub binaries: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct NpmPublishPlanArgs {
    /// Ordered npm pack --json output files: seven platforms, then main.
    #[arg(long, required = true, num_args = 1..)]
    pub pack_json: Vec<PathBuf>,
}

#[derive(Clone, Copy)]
struct Platform {
    target: &'static str,
    name: &'static str,
    os: &'static str,
    cpu: &'static str,
    libc: Option<&'static str>,
    binary: &'static str,
}

const PLATFORMS: [Platform; 7] = [
    Platform {
        target: "aarch64-apple-darwin",
        name: "darwin-arm64",
        os: "darwin",
        cpu: "arm64",
        libc: None,
        binary: "hyalo",
    },
    Platform {
        target: "x86_64-unknown-linux-gnu",
        name: "linux-x64",
        os: "linux",
        cpu: "x64",
        libc: Some("glibc"),
        binary: "hyalo",
    },
    Platform {
        target: "aarch64-unknown-linux-gnu",
        name: "linux-arm64",
        os: "linux",
        cpu: "arm64",
        libc: Some("glibc"),
        binary: "hyalo",
    },
    Platform {
        target: "x86_64-unknown-linux-musl",
        name: "linux-x64-musl",
        os: "linux",
        cpu: "x64",
        libc: Some("musl"),
        binary: "hyalo",
    },
    Platform {
        target: "aarch64-unknown-linux-musl",
        name: "linux-arm64-musl",
        os: "linux",
        cpu: "arm64",
        libc: Some("musl"),
        binary: "hyalo",
    },
    Platform {
        target: "x86_64-pc-windows-msvc",
        name: "win32-x64",
        os: "win32",
        cpu: "x64",
        libc: None,
        binary: "hyalo.exe",
    },
    Platform {
        target: "aarch64-pc-windows-msvc",
        name: "win32-arm64",
        os: "win32",
        cpu: "arm64",
        libc: None,
        binary: "hyalo.exe",
    },
];

pub fn run(args: NpmArgs) -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let version = workspace_version(&root)?;
    if let Some(out) = args.stage {
        let input = args
            .binaries
            .as_deref()
            .context("--stage requires --binaries")?;
        stage(&root, &out, input, &version)?;
        return Ok(true);
    }
    if args.generate {
        generate_metadata(&root, &version)?;
    }
    check_metadata_with_version(&root, &version)
}

pub(crate) fn check_metadata(root: &Path) -> Result<bool> {
    let version = workspace_version(root)?;
    check_metadata_with_version(root, &version)
}

pub fn run_publication_plan(args: NpmPublishPlanArgs) -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let version = workspace_version(&root)?;
    let artifacts = read_pack_artifacts(&args.pack_json)?;
    let plan = build_publication_plan(&artifacts, &version, query_npm_registry)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(true)
}

#[derive(Debug)]
struct PackArtifact {
    name: String,
    version: String,
    tarball: PathBuf,
    integrity: String,
}

#[derive(serde::Deserialize)]
struct PackEntry {
    name: String,
    version: String,
    filename: String,
    integrity: String,
}

#[derive(Debug, PartialEq)]
enum RegistryState {
    MissingVersion,
    Published(String),
}

#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum PublicationAction {
    Publish,
    Skip,
}

#[derive(Debug, serde::Serialize)]
struct PublicationPlan {
    name: String,
    version: String,
    tarball: String,
    integrity: String,
    action: PublicationAction,
}

fn read_pack_artifacts(paths: &[PathBuf]) -> Result<Vec<PackArtifact>> {
    paths
        .iter()
        .map(|path| {
            let bytes = fs::read(path)
                .with_context(|| format!("reading npm pack metadata {}", path.display()))?;
            let entries: Vec<PackEntry> = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing npm pack metadata {}", path.display()))?;
            let [entry] = entries.as_slice() else {
                bail!(
                    "npm pack metadata {} must contain exactly one entry",
                    path.display()
                );
            };
            if entry.integrity.is_empty() {
                bail!("npm pack metadata {} has no integrity", path.display());
            }
            let parent = path.parent().context("npm pack metadata has no parent")?;
            let tarball = parent.join(&entry.filename);
            if !tarball.is_file() {
                bail!("npm pack tarball is missing: {}", tarball.display());
            }
            Ok(PackArtifact {
                name: entry.name.clone(),
                version: entry.version.clone(),
                tarball,
                integrity: entry.integrity.clone(),
            })
        })
        .collect()
}

fn build_publication_plan<F>(
    artifacts: &[PackArtifact],
    version: &str,
    mut query: F,
) -> Result<Vec<PublicationPlan>>
where
    F: FnMut(&str, &str) -> Result<RegistryState>,
{
    let expected_names = PLATFORMS
        .iter()
        .map(|platform| package_name(*platform))
        .chain(std::iter::once("hyalo".to_owned()))
        .collect::<Vec<_>>();
    if artifacts.len() != expected_names.len() {
        bail!(
            "publication plan requires exactly {} ordered packages, got {}",
            expected_names.len(),
            artifacts.len()
        );
    }
    let mut plan = Vec::with_capacity(artifacts.len());
    for (artifact, expected_name) in artifacts.iter().zip(&expected_names) {
        if artifact.name != *expected_name {
            bail!(
                "publication package order mismatch: expected {expected_name}, got {}",
                artifact.name
            );
        }
        if artifact.version != version {
            bail!(
                "{} version {} differs from Cargo workspace {version}",
                artifact.name,
                artifact.version
            );
        }
        let action = match query(&artifact.name, &artifact.version)? {
            RegistryState::MissingVersion => PublicationAction::Publish,
            RegistryState::Published(integrity) if integrity == artifact.integrity => {
                PublicationAction::Skip
            }
            RegistryState::Published(integrity) => {
                bail!(
                    "{}@{} already exists with integrity {integrity}, local tarball is {}",
                    artifact.name,
                    artifact.version,
                    artifact.integrity
                );
            }
        };
        plan.push(PublicationPlan {
            name: artifact.name.clone(),
            version: artifact.version.clone(),
            tarball: artifact.tarball.to_string_lossy().into_owned(),
            integrity: artifact.integrity.clone(),
            action,
        });
    }
    Ok(plan)
}

fn query_npm_registry(name: &str, version: &str) -> Result<RegistryState> {
    #[cfg(windows)]
    const NPM: &str = "npm.cmd";
    #[cfg(not(windows))]
    const NPM: &str = "npm";
    query_npm_registry_with(Path::new(NPM), name, version)
}

fn query_npm_registry_with(npm: &Path, name: &str, version: &str) -> Result<RegistryState> {
    let package_version = format!("{name}@{version}");
    let output = Command::new(npm)
        .args([
            "view",
            &package_version,
            "dist.integrity",
            "--json",
            "--registry",
            "https://registry.npmjs.org",
        ])
        .output()
        .with_context(|| format!("querying npm registry for {package_version}"))?;
    classify_registry_output(
        &package_version,
        output.status.success(),
        &output.stdout,
        &output.stderr,
    )
}

fn classify_registry_output(
    package_version: &str,
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<RegistryState> {
    if success {
        let value: Value = serde_json::from_slice(stdout)
            .with_context(|| format!("parsing npm registry response for {package_version}"))?;
        let integrity = value
            .as_str()
            .filter(|value| !value.is_empty())
            .with_context(|| {
                format!("npm registry returned {package_version} without a valid dist.integrity")
            })?;
        return Ok(RegistryState::Published(integrity.to_owned()));
    }
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let error_code = serde_json::from_slice::<Value>(stdout)
        .ok()
        .and_then(|value| value["error"]["code"].as_str().map(str::to_owned));
    if matches!(error_code.as_deref(), Some("E401" | "E403"))
        || diagnostics.contains("E401")
        || diagnostics.contains("E403")
    {
        bail!("npm registry authorization failed for {package_version}: {diagnostics}");
    }
    if error_code.as_deref() == Some("E404") {
        return Ok(RegistryState::MissingVersion);
    }
    bail!("npm registry query failed for {package_version}: {diagnostics}")
}

fn workspace_version(root: &Path) -> Result<String> {
    let path = root.join("Cargo.toml");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading workspace manifest {}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .with_context(|| format!("parsing workspace manifest {}", path.display()))?;
    value
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .context("missing workspace.package.version")
}

fn package_name(platform: Platform) -> String {
    format!("@ractive-ch/hyalo-{}", platform.name)
}

fn platform_manifest(platform: Platform, version: &str) -> Value {
    let mut manifest = json!({
        "name": package_name(platform),
        "version": version,
        "description": "Hyalo CLI native binary for this platform",
        "license": "MIT",
        "os": [platform.os],
        "cpu": [platform.cpu],
        "files": [platform.binary, "LICENSE", "README.md"],
        "repository": {
            "type": "git",
            "url": "https://github.com/ractive/hyalo.git",
            "directory": format!("npm/platforms/{}", platform.name)
        },
        "publishConfig": {"access": "public"}
    });
    if let Some(libc) = platform.libc {
        manifest["libc"] = json!([libc]);
    }
    manifest
}

fn main_manifest(version: &str) -> Value {
    let mut optional = serde_json::Map::new();
    for platform in PLATFORMS {
        optional.insert(package_name(platform), json!(version));
    }
    json!({
        "name": "hyalo",
        "version": version,
        "description": "Hyalo knowledgebase CLI",
        "license": "MIT",
        "bin": {"hyalo": "bin/hyalo.js"},
        "files": ["bin", "lib", "README.md", "LICENSE", "platforms.json"],
        "engines": {"node": ">=22.14.0", "npm": ">=11.5.1"},
        "dependencies": {"detect-libc": "2.1.2"},
        "optionalDependencies": optional,
        "scripts": {"test": "node --test test/*.test.js"},
        "repository": {
            "type": "git",
            "url": "https://github.com/ractive/hyalo.git",
            "directory": "npm/hyalo"
        },
        "publishConfig": {"access": "public"}
    })
}

fn platform_map() -> Value {
    json!(
        PLATFORMS
            .iter()
            .map(|platform| json!({
                "target": platform.target,
                "package": package_name(*platform),
                "os": platform.os,
                "cpu": platform.cpu,
                "libc": platform.libc
            }))
            .collect::<Vec<_>>()
    )
}

fn json_bytes(value: &Value) -> Result<Vec<u8>> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?).into_bytes())
}

fn platform_readme(platform: Platform) -> Vec<u8> {
    format!(
        "# {}\n\nNative Hyalo CLI binary for {}.\n",
        package_name(platform),
        platform.target
    )
    .into_bytes()
}

fn expected_metadata(root: &Path, version: &str) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let license = fs::read(root.join("LICENSE")).context("reading repository LICENSE")?;
    let mut expected = vec![
        (
            root.join("npm/hyalo/package.json"),
            json_bytes(&main_manifest(version))?,
        ),
        (
            root.join("npm/hyalo/platforms.json"),
            json_bytes(&platform_map())?,
        ),
        (root.join("npm/hyalo/LICENSE"), license.clone()),
    ];
    for platform in PLATFORMS {
        let dir = root.join("npm/platforms").join(platform.name);
        expected.push((
            dir.join("package.json"),
            json_bytes(&platform_manifest(platform, version))?,
        ));
        expected.push((dir.join("LICENSE"), license.clone()));
        expected.push((dir.join("README.md"), platform_readme(platform)));
    }
    Ok(expected)
}

fn generate_metadata(root: &Path, version: &str) -> Result<()> {
    for (path, contents) in expected_metadata(root, version)? {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

fn generated_text_matches(actual: &[u8], expected: &[u8]) -> bool {
    fn next_byte(bytes: &[u8], index: &mut usize) -> Option<u8> {
        let byte = *bytes.get(*index)?;
        *index += 1;
        if byte == b'\r' && bytes.get(*index) == Some(&b'\n') {
            *index += 1;
            Some(b'\n')
        } else {
            Some(byte)
        }
    }

    let mut actual_index = 0;
    let mut expected_index = 0;
    loop {
        match (
            next_byte(actual, &mut actual_index),
            next_byte(expected, &mut expected_index),
        ) {
            (Some(actual), Some(expected)) if actual == expected => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn check_metadata_with_version(root: &Path, version: &str) -> Result<bool> {
    let mut ok = true;
    for (path, expected) in expected_metadata(root, version)? {
        match fs::read(&path) {
            Ok(actual) if generated_text_matches(&actual, &expected) => {}
            Ok(_) => {
                eprintln!("npm metadata mismatch: {}", display_path(root, &path));
                ok = false;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("npm metadata missing: {}", display_path(root, &path));
                ok = false;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", path.display()));
            }
        }
    }
    for path in [
        root.join("npm/hyalo/README.md"),
        root.join("npm/hyalo/bin/hyalo.js"),
        root.join("npm/hyalo/lib/resolve-platform.js"),
        root.join("npm/hyalo/lib/run-child.js"),
    ] {
        if !path.is_file() {
            eprintln!("npm package source missing: {}", display_path(root, &path));
            ok = false;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let launcher = root.join("npm/hyalo/bin/hyalo.js");
        if launcher.is_file() && fs::metadata(&launcher)?.permissions().mode() & 0o111 == 0 {
            eprintln!(
                "npm launcher is not executable: {}",
                display_path(root, &launcher)
            );
            ok = false;
        }
    }
    Ok(ok)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn stage(root: &Path, out: &Path, input: &Path, version: &str) -> Result<()> {
    validate_staging_paths(root, out, input)?;
    if out.exists() {
        bail!("staging output already exists: {}", out.display());
    }
    if !input.is_dir() {
        bail!("binary input directory does not exist: {}", input.display());
    }
    if !check_metadata_with_version(root, version)? {
        bail!("tracked npm metadata is missing or stale; run generate-npm-packages --generate");
    }
    preflight_package_sources(root)?;
    for platform in PLATFORMS {
        let source = input.join(platform.target).join(platform.binary);
        if !source.is_file() {
            bail!(
                "missing binary for {} at {}",
                platform.target,
                source.display()
            );
        }
    }

    fs::create_dir(out).with_context(|| format!("creating staging output {}", out.display()))?;
    let main = out.join("hyalo");
    fs::create_dir(&main).with_context(|| format!("creating {}", main.display()))?;
    for name in ["README.md", "LICENSE", "package.json", "platforms.json"] {
        copy_file(&root.join("npm/hyalo").join(name), &main.join(name))?;
    }
    copy_tree(&root.join("npm/hyalo/bin"), &main.join("bin"))?;
    copy_tree(&root.join("npm/hyalo/lib"), &main.join("lib"))?;
    set_executable(&main.join("bin/hyalo.js"))?;

    for platform in PLATFORMS {
        let dir = out.join(platform.name);
        fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let binary = dir.join(platform.binary);
        copy_file(&input.join(platform.target).join(platform.binary), &binary)?;
        set_executable(&binary)?;
        for name in ["package.json", "LICENSE", "README.md"] {
            copy_file(
                &root.join("npm/platforms").join(platform.name).join(name),
                &dir.join(name),
            )?;
        }
    }
    Ok(())
}

fn preflight_package_sources(root: &Path) -> Result<()> {
    for path in [
        root.join("npm/hyalo/README.md"),
        root.join("npm/hyalo/LICENSE"),
        root.join("npm/hyalo/package.json"),
        root.join("npm/hyalo/platforms.json"),
        root.join("npm/hyalo/bin/hyalo.js"),
    ] {
        if !path.is_file() {
            bail!("missing main package file {}", path.display());
        }
    }
    for path in [root.join("npm/hyalo/bin"), root.join("npm/hyalo/lib")] {
        if !path.is_dir() {
            bail!("missing main package directory {}", path.display());
        }
    }
    for platform in PLATFORMS {
        for name in ["package.json", "LICENSE", "README.md"] {
            let path = root.join("npm/platforms").join(platform.name).join(name);
            if !path.is_file() {
                bail!("missing platform package file {}", path.display());
            }
        }
    }
    Ok(())
}

fn validate_staging_paths(root: &Path, out: &Path, input: &Path) -> Result<()> {
    let root = canonicalize_allow_missing(root)?;
    let out = canonicalize_allow_missing(out)?;
    let input = canonicalize_allow_missing(input)?;
    for (label, path) in [("repository root", &root), ("binary input", &input)] {
        if out.starts_with(path) || path.starts_with(&out) {
            bail!(
                "staging output {} overlaps {label} {}",
                out.display(),
                path.display()
            );
        }
    }
    Ok(())
}

fn canonicalize_allow_missing(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut base = absolute.as_path();
    let mut suffix = Vec::<OsString>::new();
    while !base.exists() {
        suffix.push(
            base.file_name()
                .context("path has no existing ancestor")?
                .to_owned(),
        );
        base = base.parent().context("path has no existing ancestor")?;
    }
    let mut resolved = fs::canonicalize(base)
        .with_context(|| format!("canonicalizing existing path {}", base.display()))?;
    for component in suffix.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn copy_file(source: &Path, target: &Path) -> Result<()> {
    fs::copy(source, target)
        .with_context(|| format!("copying {} to {}", source.display(), target.display()))?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        bail!("missing directory {}", source.display());
    }
    fs::create_dir(target).with_context(|| format!("creating {}", target.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("reading {}", source.display()))? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&source_path, &target_path)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &target_path)?;
        } else {
            bail!("unsupported package source entry {}", source_path.display());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "1.2.3";

    fn fixture_root() -> Result<tempfile::TempDir> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{VERSION}\"\n"),
        )?;
        fs::write(root.join("LICENSE"), b"fixture license\n")?;
        fs::create_dir_all(root.join("npm/hyalo/bin/nested"))?;
        fs::create_dir_all(root.join("npm/hyalo/lib/nested"))?;
        fs::write(root.join("npm/hyalo/README.md"), b"# fixture main\n")?;
        fs::write(
            root.join("npm/hyalo/bin/hyalo.js"),
            b"#!/usr/bin/env node\n",
        )?;
        fs::write(root.join("npm/hyalo/bin/nested/helper.js"), b"bin helper\n")?;
        fs::write(
            root.join("npm/hyalo/lib/resolve-platform.js"),
            b"resolver\n",
        )?;
        fs::write(root.join("npm/hyalo/lib/run-child.js"), b"runner\n")?;
        fs::write(root.join("npm/hyalo/lib/nested/helper.js"), b"helper\n")?;
        set_executable(&root.join("npm/hyalo/bin/hyalo.js"))?;
        generate_metadata(root, VERSION)?;
        Ok(temp)
    }

    fn fixture_binaries(parent: &Path) -> Result<PathBuf> {
        let input = parent.join("binaries");
        for platform in PLATFORMS {
            let dir = input.join(platform.target);
            fs::create_dir_all(&dir)?;
            fs::write(
                dir.join(platform.binary),
                format!("binary bytes for {}", platform.target),
            )?;
        }
        Ok(input)
    }

    fn metadata_snapshot(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        expected_metadata(root, VERSION)?
            .into_iter()
            .map(|(path, _)| {
                let relative = path.strip_prefix(root)?.to_owned();
                Ok((relative, fs::read(path)?))
            })
            .collect()
    }

    fn with_crlf(bytes: &[u8]) -> Vec<u8> {
        let mut converted = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            if byte == b'\n' {
                converted.push(b'\r');
            }
            converted.push(byte);
        }
        converted
    }

    #[test]
    fn generation_is_deterministic_and_check_detects_every_manifest_version() -> Result<()> {
        let temp = fixture_root()?;
        let root = temp.path();
        let first = metadata_snapshot(root)?;
        generate_metadata(root, VERSION)?;
        assert_eq!(metadata_snapshot(root)?, first);
        assert!(check_metadata(root)?);

        let manifests = std::iter::once(root.join("npm/hyalo/package.json")).chain(
            PLATFORMS.iter().map(|platform| {
                root.join("npm/platforms")
                    .join(platform.name)
                    .join("package.json")
            }),
        );
        for manifest in manifests {
            let mut value: Value = serde_json::from_slice(&fs::read(&manifest)?)?;
            value["version"] = json!("9.9.9");
            fs::write(&manifest, json_bytes(&value)?)?;
            assert!(
                !check_metadata(root)?,
                "version drift was accepted for {}",
                manifest.display()
            );
            generate_metadata(root, VERSION)?;
        }
        Ok(())
    }

    #[test]
    fn check_rejects_missing_platform_pin_map_and_license_drift() -> Result<()> {
        let temp = fixture_root()?;
        let root = temp.path();

        let removed = root.join("npm/platforms/linux-arm64/package.json");
        fs::remove_file(&removed)?;
        assert!(!check_metadata(root)?);
        generate_metadata(root, VERSION)?;

        let main_path = root.join("npm/hyalo/package.json");
        let mut main: Value = serde_json::from_slice(&fs::read(&main_path)?)?;
        main["optionalDependencies"]["@ractive-ch/hyalo-linux-x64"] = json!("1.2.4");
        fs::write(&main_path, json_bytes(&main)?)?;
        assert!(!check_metadata(root)?);
        generate_metadata(root, VERSION)?;

        let map_path = root.join("npm/hyalo/platforms.json");
        let mut map: Value = serde_json::from_slice(&fs::read(&map_path)?)?;
        map[0]["package"] = json!("@ractive-ch/wrong");
        fs::write(&map_path, json_bytes(&map)?)?;
        assert!(!check_metadata(root)?);
        generate_metadata(root, VERSION)?;

        fs::write(root.join("npm/platforms/darwin-arm64/LICENSE"), b"stale\n")?;
        assert!(!check_metadata(root)?);
        Ok(())
    }

    #[test]
    fn check_and_stage_accept_crlf_generated_metadata_without_masking_drift() -> Result<()> {
        let temp = fixture_root()?;
        let root = temp.path();
        let generated_paths = expected_metadata(root, VERSION)?
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>();
        for path in &generated_paths {
            fs::write(path, with_crlf(&fs::read(path)?))?;
        }

        assert!(check_metadata(root)?);

        let outside = tempfile::tempdir()?;
        let input = fixture_binaries(outside.path())?;
        let output = outside.path().join("stage");
        stage(root, &output, &input, VERSION)?;
        assert!(
            fs::read(output.join("linux-x64/README.md"))?
                .windows(2)
                .any(|pair| pair == b"\r\n")
        );

        let readme = root.join("npm/platforms/linux-x64/README.md");
        fs::write(&readme, b"# wrong package\r\n")?;
        assert!(!check_metadata(root)?);

        generate_metadata(root, VERSION)?;
        fs::remove_file(root.join("npm/hyalo/platforms.json"))?;
        assert!(!check_metadata(root)?);

        assert!(generated_text_matches(b"line\r\nnext\n", b"line\nnext\r\n"));
        assert!(!generated_text_matches(b"line\rnext\n", b"line\nnext\n"));
        assert!(!generated_text_matches(b"value \xff\n", b"value \xfe\r\n"));
        Ok(())
    }

    #[test]
    fn staging_copies_complete_main_tree_and_all_platform_binaries() -> Result<()> {
        let temp = fixture_root()?;
        let root = temp.path();
        let outside = tempfile::tempdir()?;
        let input = fixture_binaries(outside.path())?;
        let output = outside.path().join("stage");

        stage(root, &output, &input, VERSION)?;
        assert_eq!(
            fs::read(output.join("hyalo/bin/nested/helper.js"))?,
            b"bin helper\n"
        );
        assert_eq!(
            fs::read(output.join("hyalo/lib/nested/helper.js"))?,
            b"helper\n"
        );
        assert_eq!(
            fs::read(output.join("hyalo/package.json"))?,
            fs::read(root.join("npm/hyalo/package.json"))?
        );
        for platform in PLATFORMS {
            let staged = output.join(platform.name).join(platform.binary);
            assert_eq!(
                fs::read(&staged)?,
                format!("binary bytes for {}", platform.target).as_bytes()
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_ne!(fs::metadata(&staged)?.permissions().mode() & 0o111, 0);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(output.join("hyalo/bin/hyalo.js"))?
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        Ok(())
    }

    #[test]
    fn staging_missing_binary_leaves_no_partial_output() -> Result<()> {
        let temp = fixture_root()?;
        let outside = tempfile::tempdir()?;
        let input = fixture_binaries(outside.path())?;
        let missing = PLATFORMS[3];
        fs::remove_file(input.join(missing.target).join(missing.binary))?;
        let output = outside.path().join("stage");

        let error = stage(temp.path(), &output, &input, VERSION)
            .expect_err("missing binary should fail staging");
        assert!(error.to_string().contains(missing.target));
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn staging_refuses_existing_output_without_touching_it() -> Result<()> {
        let temp = fixture_root()?;
        let outside = tempfile::tempdir()?;
        let input = fixture_binaries(outside.path())?;
        let output = outside.path().join("stage");
        fs::create_dir(&output)?;
        let sentinel = output.join("sentinel");
        fs::write(&sentinel, b"keep me")?;

        let error = stage(temp.path(), &output, &input, VERSION)
            .expect_err("existing staging output should be refused");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(sentinel)?, b"keep me");
        Ok(())
    }

    #[test]
    fn staging_rejects_output_overlapping_repository_or_input() -> Result<()> {
        let temp = fixture_root()?;
        let root = temp.path();
        let outside = tempfile::tempdir()?;
        let input = fixture_binaries(outside.path())?;

        let repository_output = root.join("stage");
        let error = stage(root, &repository_output, &input, VERSION)
            .expect_err("repository child should be refused");
        assert!(error.to_string().contains("repository root"));

        let input_output = input.join("stage");
        let error = stage(root, &input_output, &input, VERSION)
            .expect_err("binary-input child should be refused");
        assert!(error.to_string().contains("binary input"));
        Ok(())
    }

    fn publication_artifacts() -> Vec<PackArtifact> {
        PLATFORMS
            .iter()
            .enumerate()
            .map(|(index, platform)| PackArtifact {
                name: package_name(*platform),
                version: VERSION.to_owned(),
                tarball: PathBuf::from(format!("platform-{index}.tgz")),
                integrity: format!("sha512-local-{index}"),
            })
            .chain(std::iter::once(PackArtifact {
                name: "hyalo".to_owned(),
                version: VERSION.to_owned(),
                tarball: PathBuf::from("hyalo.tgz"),
                integrity: "sha512-local-main".to_owned(),
            }))
            .collect()
    }

    #[test]
    fn registry_response_distinguishes_missing_version_from_missing_integrity() -> Result<()> {
        assert_eq!(
            classify_registry_output("hyalo@1.2.3", true, br#""sha512-published""#, b"")?,
            RegistryState::Published("sha512-published".to_owned())
        );
        let missing_version = br#"{
          "error": {
            "code": "E404",
            "summary": "No match found for version 9999.0.0",
            "detail": ""
          }
        }"#;
        assert_eq!(
            classify_registry_output("detect-libc@9999.0.0", false, missing_version, b"")?,
            RegistryState::MissingVersion
        );
        assert!(
            classify_registry_output(
                "hyalo@1.2.3",
                false,
                b"",
                b"network proxy mentioned E404 without registry JSON"
            )
            .is_err()
        );
        let missing_integrity = classify_registry_output("hyalo@1.2.3", true, b"null", b"")
            .expect_err("a successful version lookup without integrity must fail");
        assert!(missing_integrity.to_string().contains("dist.integrity"));
        Ok(())
    }

    #[test]
    fn registry_response_fails_closed_on_auth_network_and_malformed_data() {
        for diagnostics in [
            b"npm error code E401".as_slice(),
            b"npm error code E403".as_slice(),
            b"npm error code ENETUNREACH".as_slice(),
        ] {
            assert!(classify_registry_output("hyalo@1.2.3", false, b"", diagnostics).is_err());
        }
        assert!(classify_registry_output("hyalo@1.2.3", true, b"not json", b"").is_err());
    }

    #[test]
    fn publication_plan_resumes_partial_success_in_platforms_first_order() -> Result<()> {
        let artifacts = publication_artifacts();
        let mut query_index = 0usize;
        let plan = build_publication_plan(&artifacts, VERSION, |_, _| {
            let artifact = &artifacts[query_index];
            let state = if query_index < 3 {
                RegistryState::Published(artifact.integrity.clone())
            } else {
                RegistryState::MissingVersion
            };
            query_index += 1;
            Ok(state)
        })?;
        assert_eq!(
            plan.iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            artifacts
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            plan[..3]
                .iter()
                .all(|entry| entry.action == PublicationAction::Skip)
        );
        assert!(
            plan[3..]
                .iter()
                .all(|entry| entry.action == PublicationAction::Publish)
        );
        assert_eq!(plan.last().map(|entry| entry.name.as_str()), Some("hyalo"));
        Ok(())
    }

    #[test]
    fn publication_plan_rejects_existing_version_with_different_integrity() {
        let artifacts = publication_artifacts();
        let error = build_publication_plan(&artifacts, VERSION, |_, _| {
            Ok(RegistryState::Published("sha512-other".to_owned()))
        })
        .expect_err("an immutable version mismatch must fail");
        assert!(error.to_string().contains("already exists with integrity"));
    }

    #[cfg(unix)]
    #[test]
    fn unix_registry_query_executes_offline_fixture() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let npm = temp.path().join("npm");
        fs::write(
            &npm,
            "#!/bin/sh\nprintf '%s\\n' '\"sha512-unix-fixture\"'\n",
        )?;
        set_executable(&npm)?;
        assert_eq!(
            query_npm_registry_with(&npm, "hyalo", VERSION)?,
            RegistryState::Published("sha512-unix-fixture".to_owned())
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_registry_query_executes_cmd_shim() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let npm = temp.path().join("npm.cmd");
        fs::write(
            &npm,
            "@echo off\r\necho \"sha512-windows-cmd-fixture\"\r\nexit /b 0\r\n",
        )?;
        assert_eq!(
            query_npm_registry_with(&npm, "hyalo", VERSION)?,
            RegistryState::Published("sha512-windows-cmd-fixture".to_owned())
        );
        Ok(())
    }
}
