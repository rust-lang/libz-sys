use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use tempfile::TempDir;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let program = args
        .next()
        .unwrap_or_else(|| OsString::from("maint"))
        .to_string_lossy()
        .into_owned();

    let Some(subcommand) = args.next() else {
        print_help(&program);
        return Ok(());
    };

    match subcommand.to_str() {
        Some("publish") => run_cargo_publish(args),
        Some("zng") => run_cargo_zng(args),
        Some("-h") | Some("--help") | Some("help") => {
            print_help(&program);
            Ok(())
        }
        Some(other) => bail!("unknown subcommand `{other}`"),
        None => bail!("subcommand must be valid UTF-8"),
    }
}

fn print_help(program: &str) {
    println!("Usage:\n  {program} publish [--execute]\n  {program} zng <cargo-args...>\n");
}

fn run_cargo_publish(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let mut execute = false;
    for arg in args {
        match arg.to_str() {
            Some("--execute") => execute = true,
            Some("-h") | Some("--help") => {
                println!("Usage: maint publish [--execute]");
                return Ok(());
            }
            Some(other) => bail!("unexpected publish flag `{other}`"),
            None => bail!("publish flags must be valid UTF-8"),
        }
    }

    let repo_root = env::current_dir().context("failed to determine current directory")?;
    let root_manifest = read_manifest_info(&repo_root.join(ROOT_MANIFEST))?;
    let zng_manifest = read_manifest_info(&repo_root.join(ZNG_MANIFEST))?;

    if root_manifest.name != ROOT_PACKAGE_NAME {
        bail!(
            "expected `{ROOT_PACKAGE_NAME}` in {}, found `{}`",
            ROOT_MANIFEST,
            root_manifest.name
        );
    }
    if zng_manifest.name != ZNG_PACKAGE_NAME {
        bail!(
            "expected `{ZNG_PACKAGE_NAME}` in {}, found `{}`",
            ZNG_MANIFEST,
            zng_manifest.name
        );
    }
    if root_manifest.version != zng_manifest.version {
        bail!(
            "crate versions must match before release: {} is {}, {} is {}",
            ROOT_MANIFEST,
            root_manifest.version,
            ZNG_MANIFEST,
            zng_manifest.version
        );
    }

    ensure_paths_exist(&repo_root, REQUIRED_WORKTREE_PATHS, "worktree")?;
    assert_package_contains(
        &repo_root,
        &root_manifest.name,
        LIBZ_SYS_PACKAGE_SENTINELS,
        "libz-sys package contents",
    )?;

    let staged_zng = stage_zng(&repo_root, false)?;
    assert_package_contains(
        &staged_zng.root,
        &zng_manifest.name,
        LIBZ_NG_PACKAGE_SENTINELS,
        "libz-ng-sys package contents",
    )?;

    for feature_set in [
        FeatureSet::Default,
        FeatureSet::Static,
        FeatureSet::ZlibNg,
        FeatureSet::ZlibNgNoCmake,
    ] {
        run_publish(
            &repo_root,
            &root_manifest.name,
            PublishMode::DryRun,
            Some(feature_set),
        )?;
    }
    run_publish(
        &staged_zng.root,
        &zng_manifest.name,
        PublishMode::DryRun,
        None,
    )?;

    if !execute {
        println!(
            "publish dry-run for {} {} completed; rerun with --execute to upload",
            root_manifest.name, root_manifest.version
        );
        return Ok(());
    }

    run_publish(
        &repo_root,
        &root_manifest.name,
        PublishMode::UploadAfterVerification,
        None,
    )?;
    run_publish(
        &staged_zng.root,
        &zng_manifest.name,
        PublishMode::UploadAfterVerification,
        None,
    )?;

    println!(
        "published {} and {} version {}",
        root_manifest.name, zng_manifest.name, root_manifest.version
    );
    println!("next steps:");
    println!(
        "  1. Run `git tag -s {} -m \"{} {}\"`.",
        root_manifest.version, root_manifest.name, root_manifest.version
    );
    println!("  2. Run `git push --tags origin`.");
    Ok(())
}

fn run_cargo_zng(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let cargo_args: Vec<OsString> = args.into_iter().collect();
    if cargo_args.is_empty() {
        bail!("Usage: maint zng <cargo-args...>");
    }

    if matches!(cargo_args[0].to_str(), Some("-h" | "--help")) {
        println!("Usage: maint zng <cargo-args...>");
        println!("Example: maint zng test");
        return Ok(());
    }

    let repo_root = env::current_dir().context("failed to determine current directory")?;
    let staged_zng = stage_zng(&repo_root, true)?;
    let forwarded_args = sanitize_zng_args(cargo_args)?;
    run_cargo_in_dir(&staged_zng.root, &forwarded_args)
}

fn sanitize_zng_args(mut cargo_args: Vec<OsString>) -> Result<Vec<OsString>> {
    if !matches!(
        cargo_args.first().and_then(|arg| arg.to_str()),
        Some("publish")
    ) {
        return Ok(cargo_args);
    }

    if cargo_args
        .iter()
        .any(|arg| arg == OsStr::new("--no-verify"))
    {
        bail!(
            "refusing to run `cargo publish --no-verify` through `maint zng`; use `maint publish --execute` instead"
        );
    }

    if !cargo_args
        .iter()
        .any(|arg| arg == OsStr::new("--dry-run") || arg == OsStr::new("-n"))
    {
        cargo_args.push(OsString::from("--dry-run"));
    }

    Ok(cargo_args)
}

fn stage_zng(repo_root: &Path, allow_dirty: bool) -> Result<StagedZng> {
    let package_files = package_file_list(repo_root, allow_dirty)?;
    let tempdir = tempfile::tempdir().context("failed to create temporary staging directory")?;
    let staging_root = tempdir.path().to_path_buf();

    for relative_path in package_files {
        if PACKAGE_LIST_EXCLUDE
            .iter()
            .any(|excluded| relative_path == Path::new(excluded))
        {
            continue;
        }

        let source = repo_root.join(&relative_path);
        let destination = staging_root.join(&relative_path);
        copy_path(&source, &destination)
            .with_context(|| format!("failed to stage {}", relative_path.display()))?;
    }

    copy_file(
        &repo_root.join(ZNG_MANIFEST),
        &staging_root.join(ROOT_MANIFEST),
    )?;
    copy_dir_recursive(
        &repo_root.join(SYSTEST_DIR),
        &staging_root.join(SYSTEST_DIR),
    )?;

    let staged_systest_manifest = staging_root.join(SYSTEST_MANIFEST);
    if staged_systest_manifest.exists() {
        fs::remove_file(&staged_systest_manifest)
            .with_context(|| format!("failed to remove {}", staged_systest_manifest.display()))?;
    }
    fs::rename(
        staging_root.join(SYSTEST_ZNG_MANIFEST),
        staging_root.join(SYSTEST_MANIFEST),
    )
    .with_context(|| {
        format!(
            "failed to activate {} in staged systest workspace",
            SYSTEST_ZNG_MANIFEST
        )
    })?;

    Ok(StagedZng {
        _tempdir: tempdir,
        root: staging_root,
    })
}

fn package_file_list(directory: &Path, allow_dirty: bool) -> Result<Vec<PathBuf>> {
    let mut command = Command::new("cargo");
    command.current_dir(directory).arg("package").arg("--list");
    if allow_dirty {
        command.arg("--allow-dirty");
    }
    let output = run_and_capture(&mut command)?;

    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(PathBuf::from(line)))
        .collect()
}

fn ensure_paths_exist(root: &Path, required_paths: &[&str], label: &str) -> Result<()> {
    for relative_path in required_paths {
        let absolute_path = root.join(relative_path);
        if !absolute_path.exists() {
            bail!(
                "missing required {label} path `{relative_path}`; check that all release submodules are initialized"
            );
        }
    }
    Ok(())
}

fn assert_package_contains(
    directory: &Path,
    package_name: &str,
    required_paths: &[&str],
    label: &str,
) -> Result<()> {
    let listed_files = package_file_list(directory, false)?;
    for relative_path in required_paths {
        let present = listed_files
            .iter()
            .any(|listed| listed == Path::new(relative_path));
        if !present {
            bail!(
                "missing `{relative_path}` in {label} for `{package_name}`; refusing to continue"
            );
        }
    }
    Ok(())
}

fn run_publish(
    directory: &Path,
    package_name: &str,
    mode: PublishMode,
    feature_set: Option<FeatureSet>,
) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .current_dir(directory)
        .arg("publish")
        .arg("--package")
        .arg(package_name);
    mode.append_args(&mut command);
    if let Some(feature_set) = feature_set {
        feature_set.append_publish_args(&mut command);
        eprintln!("verifying `{package_name}` with {}", feature_set.describe());
    } else {
        match mode {
            PublishMode::DryRun => eprintln!("verifying `{package_name}` with default features"),
            PublishMode::UploadAfterVerification => eprintln!(
                "uploading `{package_name}` with --no-verify after successful dry-run verification"
            ),
        }
    }
    run_command(&mut command)
}

fn run_cargo_in_dir(directory: &Path, cargo_args: &[OsString]) -> Result<()> {
    let mut command = Command::new("cargo");
    command.current_dir(directory).args(cargo_args);
    run_command(&mut command)
}

fn run_command(command: &mut Command) -> Result<()> {
    eprintln!("+ {}", format_command(command));
    let status = command
        .status()
        .with_context(|| format!("failed to spawn {}", format_command(command)))?;
    if status.success() {
        Ok(())
    } else {
        bail!("command failed: {} ({status})", format_command(command));
    }
}

fn run_and_capture(command: &mut Command) -> Result<String> {
    eprintln!("+ {}", format_command(command));
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to spawn {}", format_command(command)))?;
    if !output.status.success() {
        bail!(
            "command failed: {} ({})",
            format_command(command),
            output.status
        );
    }
    String::from_utf8(output.stdout).context("command output was not valid UTF-8")
}

fn format_command(command: &Command) -> String {
    let mut rendered = Vec::new();
    rendered.push(command.get_program().to_string_lossy().into_owned());
    rendered.extend(
        command
            .get_args()
            .map(|arg| shell_escape(arg.to_string_lossy().as_ref())),
    );
    rendered.join(" ")
}

fn shell_escape(argument: &str) -> String {
    if argument.is_empty() {
        return "''".to_string();
    }
    if argument
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '='))
    {
        return argument.to_string();
    }
    format!("'{}'", argument.replace('\'', "'\"'\"'"))
}

fn read_manifest_info(path: &Path) -> Result<ManifestInfo> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    let document: toml::Value =
        toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))?;
    let package = document
        .get("package")
        .and_then(|value| value.as_table())
        .ok_or_else(|| anyhow!("{} is missing a [package] table", path.display()))?;
    let name = package
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("{} is missing package.name", path.display()))?;
    let version = package
        .get("version")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("{} is missing package.version", path.display()))?;
    Ok(ManifestInfo {
        name: name.to_owned(),
        version: version.to_owned(),
    })
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        copy_dir_recursive(source, destination)
    } else if source.is_file() {
        copy_file(source, destination)
    } else {
        bail!("{} does not exist", source.display());
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", source.display()))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        copy_path(&source_path, &destination_path)?;
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    let permissions = fs::metadata(source)
        .with_context(|| format!("failed to read metadata for {}", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("failed to set permissions for {}", destination.display()))?;
    Ok(())
}

const ROOT_MANIFEST: &str = "Cargo.toml";
const ZNG_MANIFEST: &str = "Cargo-zng.toml";
const SYSTEST_DIR: &str = "systest";
const SYSTEST_MANIFEST: &str = "systest/Cargo.toml";
const SYSTEST_ZNG_MANIFEST: &str = "systest/Cargo-zng.toml";
const ROOT_PACKAGE_NAME: &str = "libz-sys";
const ZNG_PACKAGE_NAME: &str = "libz-ng-sys";
const PACKAGE_LIST_EXCLUDE: &[&str] = &[".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml.orig"];
const REQUIRED_WORKTREE_PATHS: &[&str] = &[
    "src/zlib/adler32.c",
    "src/zlib/zlib.h",
    "src/zlib-ng/adler32.c",
    "src/zlib-ng/CMakeLists.txt",
];
const LIBZ_SYS_PACKAGE_SENTINELS: &[&str] = &[
    "build.rs",
    "src/zlib/adler32.c",
    "src/zlib/zlib.h",
    "src/zlib-ng/adler32.c",
    "src/zlib-ng/CMakeLists.txt",
    "zng/cmake.rs",
];
const LIBZ_NG_PACKAGE_SENTINELS: &[&str] = &[
    "README-zng.md",
    "src/lib.rs",
    "src/zlib-ng/adler32.c",
    "src/zlib-ng/CMakeLists.txt",
    "zng/cmake.rs",
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct ManifestInfo {
    name: String,
    version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FeatureSet {
    Default,
    Static,
    ZlibNg,
    ZlibNgNoCmake,
}

impl FeatureSet {
    fn describe(self) -> &'static str {
        match self {
            Self::Default => "default features",
            Self::Static => "--features static",
            Self::ZlibNg => "--no-default-features --features zlib-ng",
            Self::ZlibNgNoCmake => {
                "--no-default-features --features zlib-ng-no-cmake-experimental-community-maintained"
            }
        }
    }

    fn append_publish_args(self, command: &mut Command) {
        match self {
            Self::Default => {}
            Self::Static => {
                command.arg("--features").arg("static");
            }
            Self::ZlibNg => {
                command
                    .arg("--no-default-features")
                    .arg("--features")
                    .arg("zlib-ng");
            }
            Self::ZlibNgNoCmake => {
                command
                    .arg("--no-default-features")
                    .arg("--features")
                    .arg("zlib-ng-no-cmake-experimental-community-maintained");
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishMode {
    DryRun,
    /// The upload step intentionally uses `cargo publish --no-verify` because
    /// this tool already verified the release with the full dry-run feature
    /// matrix. Running Cargo's built-in verify again here would be narrower and
    /// redundant.
    UploadAfterVerification,
}

impl PublishMode {
    fn append_args(self, command: &mut Command) {
        match self {
            Self::DryRun => {
                command.arg("--dry-run");
            }
            Self::UploadAfterVerification => {
                command.arg("--no-verify");
            }
        }
    }
}

struct StagedZng {
    _tempdir: TempDir,
    root: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_mode_appends_safe_flags() {
        let mut command = Command::new("cargo");
        command.arg("publish");
        PublishMode::DryRun.append_args(&mut command);
        assert!(command.get_args().any(|arg| arg == OsStr::new("--dry-run")));

        let mut command = Command::new("cargo");
        command.arg("publish");
        PublishMode::UploadAfterVerification.append_args(&mut command);
        assert!(
            command
                .get_args()
                .any(|arg| arg == OsStr::new("--no-verify"))
        );
    }

    #[test]
    fn feature_sets_append_expected_publish_flags() {
        let mut command = Command::new("cargo");
        command.arg("publish");
        FeatureSet::ZlibNg.append_publish_args(&mut command);
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            vec![
                OsStr::new("publish"),
                OsStr::new("--no-default-features"),
                OsStr::new("--features"),
                OsStr::new("zlib-ng"),
            ]
        );
    }

    #[test]
    fn zng_publish_defaults_to_dry_run() {
        let sanitized = sanitize_zng_args(vec![OsString::from("publish")]).unwrap();
        assert_eq!(
            sanitized,
            vec![OsString::from("publish"), OsString::from("--dry-run")]
        );
    }

    #[test]
    fn zng_publish_rejects_no_verify() {
        let error = sanitize_zng_args(vec![
            OsString::from("publish"),
            OsString::from("--no-verify"),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("--no-verify"));
    }

    #[test]
    fn manifest_parser_reads_package_name_and_version() {
        let tempdir = tempfile::tempdir().unwrap();
        let manifest_path = tempdir.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            "[package]\nname = \"example\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        let manifest = read_manifest_info(&manifest_path).unwrap();
        assert_eq!(
            manifest,
            ManifestInfo {
                name: "example".to_string(),
                version: "1.2.3".to_string(),
            }
        );
    }
}
