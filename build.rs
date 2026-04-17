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
use toml::Table;

const HMCL_EXPECTED_JAVA_MAJOR_VERSION: &str = "17";

fn main() {
    let version = ProjectVersion::get();

    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=resources/HMCL.ico");
    println!("cargo:rerun-if-changed=resources/HMCL.rc");
    println!("cargo:rustc-env=HMCL_LAUNCHER_VERSION={}", version.version);
    println!(
        "cargo:rustc-env=HMCL_EXPECTED_JAVA_MAJOR_VERSION_STR={}",
        HMCL_EXPECTED_JAVA_MAJOR_VERSION
    );

    embed_resource::compile(
        "resources/HMCL.rc",
        &[
            format!("PROJECT_VERSION=\"{}\"", &version.version),
            format!("PROJECT_VERSION_MAJOR={}", &version.major),
            format!("PROJECT_VERSION_MINOR={}", &version.minor),
            format!("PROJECT_VERSION_PATCH={}", &version.patch),
            format!("PROJECT_VERSION_TWEAK={}", &version.tweak),
        ],
    )
    .manifest_optional()
    .unwrap();
}

struct ProjectVersion {
    version: String,
    major: String,
    minor: String,
    patch: String,
    tweak: String,
}

impl ProjectVersion {
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
