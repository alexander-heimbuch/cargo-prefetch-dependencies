use clap::{crate_version, App, AppSettings, Arg, SubCommand};
use failure::Fallible;
use semver::VersionReq;
use serde_derive::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use toml::value::Table;

const TEMP_PROJ_NAME: &str = "temp_prefetch_project";

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        for cause in e.iter_causes() {
            eprintln!("Caused by: {}", cause);
        }
        std::process::exit(1);
    }
}

type CrateSet = HashSet<(String, String)>;

fn run() -> Fallible<()> {
    let app_matches = App::new("cargo-prefetch-dependencies")
        .version(crate_version!())
        .bin_name("cargo")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .global_settings(&[
            AppSettings::GlobalVersion, // subcommands inherit version
            AppSettings::ColoredHelp,
            AppSettings::DeriveDisplayOrder,
        ])
        .subcommand(
            SubCommand::with_name("prefetch-dependencies")
                .about("Prefetch dependencies of cargo manifests.")
                .arg(
                    Arg::with_name("output")
                        .short("o")
                        .long("output")
                        .help("Output path")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("manifest")
                        .required(true)
                        .takes_value(true)
                        .multiple(true),
                ),
        )
        .get_matches();

    let matches = app_matches
        .subcommand_matches("prefetch-dependencies")
        .expect("Expected `prefetch-dependencies` subcommand.");

    let manifests = matches.values_of("manifest").unwrap();

    // Default behavior with no command-line options.
    let mut crates: CrateSet = HashSet::new();

    for manifest_path in manifests {
        let result: PackageDependencies = manifest_dependencies(manifest_path).unwrap();

        for dep in result.dependencies {
            crates.insert((dep.name, dep.version));
        }
        for dep in result.dev_dependencies {
            crates.insert((dep.name, dep.version));
        }
    }

    make_project(Path::new(matches.value_of("output").unwrap()), &crates)
}

/// Create a temporary Cargo project with the given dependencies.
fn make_project(path: &Path, crates: &CrateSet) -> Fallible<()> {
    let deps: Vec<String> = crates
        .iter()
        .map(|(name, version)| format!("\"{}\" = \"{}\"\n", name, version))
        .collect();

    fs::write(
        path.join("Cargo.toml"),
        format!(
            r#"
            [package]
            name = "{}"
            version = "0.0.0"

            [dependencies]
            {}
            "#,
            TEMP_PROJ_NAME,
            deps.join("")
        ),
    )?;
    fs::create_dir(path.join("src"))?;
    fs::write(path.join("src").join("lib.rs"), "")?;
    Ok(())
}

#[derive(Deserialize, Debug, Clone)]
struct Package {
    name: String,
    version: String,
}

#[derive(Deserialize, Debug, Clone)]
struct PackageDependencies {
    dev_dependencies: Vec<Package>,
    dependencies: Vec<Package>,
}

fn transform_dependencies(deps: &toml::map::Map<String, toml::Value>) -> Vec<Package> {
    let mut result = Vec::new();

    for (key, val) in deps.iter() {
        let version = if val.is_str() {
            val.as_str()
        } else {
            let value = val.as_table().unwrap();
            value.get("version").unwrap().as_str()
        }
        .unwrap();

        if VersionReq::parse(&version).is_ok() {
            let dependency = Package {
                name: key.to_string(),
                version: version.to_string(),
            };

            result.push(dependency);
        }
    }

    return result;
}

/// Return the top downloaded crates by querying crates.io.
fn manifest_dependencies(manifest: &str) -> Fallible<PackageDependencies> {
    let path = std::path::Path::new(manifest);
    let manifest_content = match std::fs::read_to_string(path) {
        Ok(f) => f,
        Err(e) => panic!("{}", e),
    };

    let mut dependencies = Vec::new();
    let mut dev_dependencies = Vec::new();

    let data: Table = manifest_content.parse().unwrap();
    let manifest_dependencies = data.get("dependencies");

    if manifest_dependencies.is_some() {
        let deps: &toml::map::Map<String, toml::Value> =
            manifest_dependencies.unwrap().as_table().unwrap();
        dependencies = transform_dependencies(deps);
    }

    let manifest_dev_dependencies = data.get("dev-dependencies");

    if manifest_dev_dependencies.is_some() {
        let dev_deps: &toml::map::Map<String, toml::Value> =
            manifest_dev_dependencies.unwrap().as_table().unwrap();
        dev_dependencies = transform_dependencies(dev_deps);
    }

    Ok(PackageDependencies {
        dependencies,
        dev_dependencies,
    })
}
