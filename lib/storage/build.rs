use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

fn get_git_sha() {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("Failed to execute git command");

    let git_hash = String::from_utf8(output.stdout).expect("Failed to convert git output to UTF-8");

    println!("cargo:rustc-env=CARGO_GIT_SHA={}", git_hash.trim());
    println!("cargo:rerun-if-changed=.git");
}

fn main() {
    get_git_sha();

    let repo = format!("{}/ocsf-schema", std::env::var("OUT_DIR").unwrap());

    // Clone or update the repository
    if !Path::new(&repo).exists() {
        Command::new("git")
            .args([
                "clone",
                "https://github.com/ocsf/ocsf-schema",
                "--depth",
                "1",
                "--branch",
                option_env!("OCSF_SCHEMA_VERSION").unwrap_or("1.4.0"),
                &repo,
            ])
            .status()
            .expect("Failed to clone repository");
    }

    // Read categories.json
    let categories_content = fs::read_to_string(format!("{}/categories.json", &repo))
        .expect("Failed to read categories.json");

    let categories: Value =
        serde_json::from_str(&categories_content).expect("Failed to parse categories.json");

    let mut output = String::from("// Auto-generated OCSF classes & categories\n\n");
    output.push_str("use num_enum::TryFromPrimitive;\n\n");

    // Generate Category enum
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive)]\n");
    output.push_str("#[repr(u32)]\n");
    output.push_str("pub enum Category {\n");
    output.push_str("    Other = 0,\n");
    if let Some(attrs) = categories["attributes"].as_object() {
        for (_, cat) in attrs {
            let caption = cat["caption"].as_str().unwrap();
            let uid = cat["uid"].as_u64().unwrap();
            let enum_name = caption.replace(" ", "");
            let enum_name = Regex::new(r"[^a-zA-Z]")
                .unwrap()
                .replace_all(&enum_name, "")
                .to_string();
            output.push_str(&format!("    {} = {},\n", enum_name, uid));
        }
    }
    output.push_str("}\n\n");

    // Generate ToString implementation for Category
    output.push_str("impl ToString for Category {\n");
    output.push_str("    fn to_string(&self) -> String {\n");
    output.push_str("        match self {\n");
    output.push_str("            Category::Other => String::from(\"other\"),\n");
    if let Some(attrs) = categories["attributes"].as_object() {
        for (cat_name, cat) in attrs {
            let caption = cat["caption"].as_str().unwrap();
            let enum_name = caption.replace(" ", "");
            let enum_name = Regex::new(r"[^a-zA-Z]")
                .unwrap()
                .replace_all(&enum_name, "")
                .to_string();
            output.push_str(&format!(
                "            Category::{} => String::from(\"{}\"),\n",
                enum_name, cat_name
            ));
        }
    }
    output.push_str("        }\n    }\n}\n\n");

    // Generate Class
    output.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive)]\n");
    output.push_str("#[repr(u32)]\n");
    output.push_str("pub enum Class {\n");
    output.push_str("    BaseEvent = 0,\n");
    if let Some(attrs) = categories["attributes"].as_object() {
        for (cat_name, cat) in attrs {
            let cat_uid = cat["uid"].as_u64().unwrap();
            let events_dir = format!("{}/events/{}", &repo, cat_name);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        let event_caption = event["caption"].as_str().unwrap();
                        if let Some(event_uid) = event["uid"].as_u64() {
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();
                            let combined_uid = cat_uid * 1000 + event_uid;

                            output.push_str(&format!(
                                "    {} = {},\n",
                                event_enum_name, combined_uid
                            ));
                        }
                    }
                }
            }

            // this got away from me
            // extensions should be handled better
            let events_dir = format!("{}/extensions/windows/events", &repo);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        if !event["extends"].as_str().unwrap().starts_with(cat_name) {
                            continue;
                        }

                        let event_caption = event["caption"].as_str().unwrap();
                        if let Some(event_uid) = event["uid"].as_u64() {
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();
                            let combined_uid = 200 * 1000 + cat_uid * 1000 + event_uid;

                            output.push_str(&format!(
                                "    {} = {},\n",
                                event_enum_name, combined_uid
                            ));
                        }
                    }
                }
            }
        }
    }
    output.push_str("}\n\n");

    // Generate ToString implementation for Class
    output.push_str("impl ToString for Class {\n");
    output.push_str("    fn to_string(&self) -> String {\n");
    output.push_str("        match self {\n");
    output.push_str("            Class::BaseEvent => String::from(\"base_event\"),\n");
    if let Some(attrs) = categories["attributes"].as_object() {
        for (cat_name, _) in attrs {
            let events_dir = format!("{}/events/{}", &repo, cat_name);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        if event["uid"].as_u64().is_some() {
                            let event_caption = event["caption"].as_str().unwrap();
                            let event_name = event["name"].as_str().unwrap();
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();

                            output.push_str(&format!(
                                "            Class::{} => String::from(\"{}\"),\n",
                                event_enum_name, event_name
                            ));
                        }
                    }
                }
            }
            let events_dir = format!("{}/extensions/windows/events", &repo);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        if !event["extends"].as_str().unwrap().starts_with(cat_name) {
                            continue;
                        }

                        if event["uid"].as_u64().is_some() {
                            let event_caption = event["caption"].as_str().unwrap();
                            let event_name = event["name"].as_str().unwrap();
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();

                            output.push_str(&format!(
                                "            Class::{} => String::from(\"win/{}\"),\n",
                                event_enum_name, event_name
                            ));
                        }
                    }
                }
            }
        }
    }
    output.push_str("        }\n    }\n}\n\n");

    // Generate FromStr implementation for Class
    output.push_str("impl std::str::FromStr for Class {\n");
    output.push_str("    type Err = String;\n");
    output.push_str("    fn from_str(s: &str) -> Result<Self, Self::Err> {\n");
    output.push_str("        match s {\n");
    output.push_str("            \"base_event\" => Ok(Class::BaseEvent),\n");
    if let Some(attrs) = categories["attributes"].as_object() {
        for (cat_name, _) in attrs {
            let events_dir = format!("{}/events/{}", &repo, cat_name);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        if event["uid"].as_u64().is_some() {
                            let event_caption = event["caption"].as_str().unwrap();
                            let event_name = event["name"].as_str().unwrap();
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();

                            output.push_str(&format!(
                                "            \"{}\" => Ok(Class::{}),\n",
                                event_name, event_enum_name
                            ));
                        }
                    }
                }
            }
            let events_dir = format!("{}/extensions/windows/events", &repo);
            if Path::new(&events_dir).exists() {
                for entry in fs::read_dir(&events_dir).unwrap() {
                    let entry = entry.unwrap();
                    if entry.path().extension().unwrap_or_default() == "json" {
                        let event_content = fs::read_to_string(entry.path()).unwrap();
                        let event: Value = serde_json::from_str(&event_content).unwrap();

                        if !event["extends"].as_str().unwrap().starts_with(cat_name) {
                            continue;
                        }

                        if event["uid"].as_u64().is_some() {
                            let event_caption = event["caption"].as_str().unwrap();
                            let event_name = event["name"].as_str().unwrap();
                            let event_enum_name = event_caption.replace(" ", "");
                            let event_enum_name = Regex::new(r"[^a-zA-Z]")
                                .unwrap()
                                .replace_all(&event_enum_name, "")
                                .to_string();

                            output.push_str(&format!(
                                "            \"win/{}\" => Ok(Class::{}),\n",
                                event_name, event_enum_name
                            ));
                        }
                    }
                }
            }
        }
    }
    output.push_str("            _ => Err(format!(\"Invalid class: {}\", s)),\n");
    output.push_str("        }\n    }\n}\n");

    // Write the generated code to ocsf_category.rs
    fs::write(
        format!("{}/ocsf.rs", std::env::var("OUT_DIR").unwrap()),
        output,
    )
    .expect("Failed to write output file");
}
