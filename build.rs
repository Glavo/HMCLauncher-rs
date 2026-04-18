/*
 * Copyright (C) 2025 Glavo. All rights reserved.
 * DO NOT ALTER OR REMOVE COPYRIGHT NOTICES OR THIS FILE HEADER.
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Table;

/// Point at the launcher's Windows resource script.
const RESOURCE_FILE: &str = "resources/HMCL.rc";

/// Generate version metadata for the Windows resource script and expose it to
/// the crate as build-time environment variables.
fn main() {
    let version = ProjectVersion::get();
    let macros = [
        format!("PROJECT_VERSION=\"{}\"", &version.version),
        format!("PROJECT_VERSION_MAJOR={}", &version.major),
        format!("PROJECT_VERSION_MINOR={}", &version.minor),
        format!("PROJECT_VERSION_PATCH={}", &version.patch),
        format!("PROJECT_VERSION_TWEAK={}", &version.tweak),
    ];
    let target = std::env::var("TARGET").expect("TARGET is not set by Cargo");

    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=resources/HMCL.ico");
    println!("cargo:rerun-if-changed={RESOURCE_FILE}");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=RC_{target}");
    println!("cargo:rerun-if-env-changed=RC_{}", target.replace('-', "_"));
    println!("cargo:rustc-env=HMCL_LAUNCHER_VERSION={}", version.version);

    if target.ends_with("-windows-msvc") {
        embed_resource::compile(RESOURCE_FILE, &macros)
            .manifest_optional()
            .unwrap();
    } else if target.ends_with("-windows-gnu") || target.ends_with("-windows-gnullvm") {
        match compile_gnu_resource(&target, RESOURCE_FILE, &macros) {
            Ok(Some(output)) => println!("cargo:rustc-link-arg-bins={}", output.display()),
            Ok(None) => {}
            Err(err) => panic!("{err}"),
        }
    }
}

/// Compile the resource script with a GNU-compatible resource compiler and
/// link the resulting COFF object into all binaries.
fn compile_gnu_resource(
    target: &str,
    resource_file: &str,
    macros: &[String],
) -> Result<Option<PathBuf>, String> {
    let compiler = match find_gnu_resource_compiler(target) {
        Some(compiler) => compiler,
        None => {
            println!(
                "cargo:warning=Skipping Windows resources for {target}: no GNU resource compiler found"
            );
            return Ok(None);
        }
    };

    let out_dir = std::env::var("OUT_DIR").map_err(|err| format!("missing OUT_DIR: {err}"))?;
    let prefix = Path::new(resource_file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("resource file path is not valid UTF-8: {resource_file}"))?;
    let out_file = Path::new(&out_dir).join(format!("{prefix}.o"));
    let include_dir = Path::new(resource_file)
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let mut command = Command::new(&compiler);
    command
        .arg("--input")
        .arg(resource_file)
        .arg("--output-format=coff")
        .arg("--target")
        .arg(windres_target(target))
        .arg("--output")
        .arg(&out_file)
        .arg("--include-dir")
        .arg(include_dir);

    // Pass the same version macros to windres that the MSVC resource path sees.
    for define in macros {
        command.arg("-D").arg(define);
    }

    let status = command
        .status()
        .map_err(|err| format!("failed to execute {}: {err}", compiler.display()))?;

    if !status.success() {
        return Err(format!(
            "{} failed to compile {resource_file} with {status}",
            compiler.display()
        ));
    }

    Ok(Some(out_file))
}

/// Resolve the GNU resource compiler from Cargo-style overrides or common
/// MinGW executable names.
fn find_gnu_resource_compiler(target: &str) -> Option<PathBuf> {
    let target_var = format!("RC_{target}");
    let normalized_target_var = format!("RC_{}", target.replace('-', "_"));

    for key in [&target_var, &normalized_target_var, "RC"] {
        if let Some(value) = std::env::var_os(key) {
            return Some(PathBuf::from(value));
        }
    }

    let arch = target.split('-').next().unwrap_or("i686");
    for candidate in [format!("{arch}-w64-mingw32-windres"), "windres".to_string()] {
        if is_command_available(&candidate) {
            return Some(PathBuf::from(candidate));
        }
    }

    None
}

/// Map Rust target triples to the architecture names understood by GNU
/// `windres`.
fn windres_target(target: &str) -> &'static str {
    if target.starts_with("x86_64-") {
        "pe-x86-64"
    } else if target.starts_with("aarch64-") {
        "pe-aarch64-little"
    } else {
        "pe-i386"
    }
}

/// Probe whether an executable can be started from the current PATH.
fn is_command_available(command: &str) -> bool {
    Command::new(command)
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Hold the four-part semantic version split that the Windows resource script
/// expects.
struct ProjectVersion {
    version: String,
    major: String,
    minor: String,
    patch: String,
    tweak: String,
}

impl ProjectVersion {
    /// Read the package version from `Cargo.toml` and split it into the four
    /// numeric components expected by the resource script.
    fn get() -> ProjectVersion {
        let project_properties = fs::read_to_string("Cargo.toml")
            .unwrap()
            .parse::<Table>()
            .unwrap();

        let package_properties = project_properties["package"].as_table().unwrap();
        let project_version = package_properties["version"]
            .as_str()
            .unwrap()
            .replace("+", ".");

        let parts = project_version.split('.').collect::<Vec<&str>>();
        if parts.len() == 4 {
            ProjectVersion {
                version: project_version.clone(),
                major: parts[0].to_string(),
                minor: parts[1].to_string(),
                patch: parts[2].to_string(),
                tweak: parts[3].to_string(),
            }
        } else {
            panic!("Cargo.toml has invalid format")
        }
    }
}
