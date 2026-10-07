//! Rule files on disk: the enabled flag, safe file names, and safe writes.
//!
//! A rule's enabled state is in its own YAML, as a top-level `enabled: false`
//! line. A rule with no such line is enabled. rsigma keeps a top-level key that
//! is not standard Sigma in [`SigmaRule::custom_attributes`], so the engine
//! reads the flag from there and skips a disabled rule when it loads the rules.
//!
//! Each write goes first to `<file>.bak`, then the service copies the `.bak`
//! file over `<file>`. Thus a full copy of the new content is on disk before
//! the rule file changes, and a crash during the copy leaves a full `.bak` to
//! recover from. rsigma loads only `.yml` and `.yaml` files, so it ignores the
//! `.bak` files.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rsigma_parser::{SigmaRule, parse_sigma_yaml};

/// The top-level YAML key that holds a rule's enabled state.
pub(crate) const ENABLED_KEY: &str = "enabled";

/// The suffix of the backup file that each write makes first.
pub(crate) const BACKUP_SUFFIX: &str = ".bak";

/// Tells if a rule is enabled. Only an explicit `enabled: false` (or the
/// string `"false"`) disables a rule.
pub(crate) fn rule_enabled(rule: &SigmaRule) -> bool {
    match rule.custom_attributes.get(ENABLED_KEY) {
        Some(v) => v.as_bool().unwrap_or_else(|| v.as_str() != Some("false")),
        None => true,
    }
}

/// Checks that a rule id is safe to use as a file name. The id comes from
/// uploaded YAML, so a value such as `../x` must not leave the rules directory.
pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "rule id '{id}' cannot be a file name (use letters, digits, '-', '_', and '.')"
        ))
    }
}

/// The path of the backup file for `path`: `<path>.bak`.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(BACKUP_SUFFIX);
    PathBuf::from(name)
}

/// Writes `contents` to `path` through its backup file. It writes and syncs
/// `<path>.bak` first, then copies it over `path` and syncs `path`. The `.bak`
/// file stays, as a full copy of the last write.
pub(crate) fn write_rule_file(path: &Path, contents: &str) -> std::io::Result<()> {
    let backup = backup_path(path);
    {
        let mut file = File::create(&backup)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::copy(&backup, path)?;
    OpenOptions::new().write(true).open(path)?.sync_all()?;
    sync_dir(path);
    Ok(())
}

/// Syncs the directory of `path`, so that a new file name is on disk too. This
/// is best effort: not all platforms can open a directory to sync it.
fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent()
        && let Ok(dir) = File::open(dir)
    {
        let _ = dir.sync_all();
    }
}

/// Removes `path` and its backup file. A file that is not there is not an
/// error.
pub(crate) fn remove_rule_file(path: &Path) -> std::io::Result<()> {
    for p in [path.to_path_buf(), backup_path(path)] {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Sets the enabled state of the rule `id` in the YAML text of a rule file.
///
/// The function edits the text, so the comments and the format of the file stay
/// the same. In the YAML document whose top-level `id:` is `id`, it removes any
/// top-level `enabled:` line. To disable the rule, it then adds
/// `enabled: false` after the `id:` line. A file can hold more than one
/// document (separated by `---`). The function changes only the document of
/// this rule.
///
/// Then it parses the result. If the rule's state is not the requested state,
/// or a rule went missing, the function gives an error and the caller writes
/// nothing.
pub(crate) fn set_enabled_in_yaml(text: &str, id: &str, enabled: bool) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    // The line range of each YAML document.
    let mut docs: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for (i, line) in lines.iter().enumerate() {
        if is_doc_separator(line) {
            docs.push((start, i));
            start = i + 1;
        }
    }
    docs.push((start, lines.len()));

    let (doc_start, doc_end) = docs
        .into_iter()
        .find(|&(s, e)| lines[s..e].iter().any(|l| top_level_value(l, "id") == Some(id)))
        .ok_or_else(|| format!("rule {id} has no top-level `id:` line in its file"))?;

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        let in_doc = (doc_start..doc_end).contains(&i);
        if in_doc && top_level_value(line, ENABLED_KEY).is_some() {
            continue;
        }
        out.push((*line).to_string());
        if in_doc && !enabled && top_level_value(line, "id") == Some(id) {
            if !line.ends_with('\n') {
                out.last_mut().unwrap().push('\n');
            }
            out.push(format!("{ENABLED_KEY}: false\n"));
        }
    }
    let edited = out.concat();

    verify_edit(text, &edited, id, enabled)?;
    Ok(edited)
}

/// Checks that an edit changed only the state of rule `id`.
fn verify_edit(before: &str, after: &str, id: &str, enabled: bool) -> Result<(), String> {
    let parse = |text: &str| {
        parse_sigma_yaml(text).map_err(|e| format!("rule file does not parse after the edit: {e}"))
    };
    let (before, after) = (parse(before)?, parse(after)?);
    if before.rules.len() != after.rules.len()
        || before.correlations.len() != after.correlations.len()
    {
        return Err("the edit changed the rules in the file".to_string());
    }
    let rule = after
        .rules
        .iter()
        .find(|r| r.id.as_deref() == Some(id))
        .ok_or_else(|| format!("rule {id} is missing after the edit"))?;
    if rule_enabled(rule) != enabled {
        return Err(format!("cannot set the enabled state of rule {id} in its file"));
    }
    Ok(())
}

/// Tells if a line is a YAML document separator (`---`).
fn is_doc_separator(line: &str) -> bool {
    let line = line.trim_end();
    line == "---" || line.starts_with("--- ")
}

/// Gets the value of a top-level `key: value` line. A top-level line starts in
/// column 0. The value has its quotes and any trailing comment removed.
fn top_level_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start_matches([' ', '\t']);
    let value = rest.strip_prefix(':')?;
    let value = match value.find(" #") {
        Some(i) => &value[..i],
        None => value,
    };
    let value = value.trim();
    Some(
        value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "\
title: Whoami # a comment that must stay
id: 11111111-1111-1111-1111-111111111111
status: test
logsource:
    product: windows
detection:
    sel:
        CommandLine|contains: whoami
    condition: sel
";

    fn parsed_enabled(text: &str, id: &str) -> bool {
        let c = parse_sigma_yaml(text).unwrap();
        rule_enabled(c.rules.iter().find(|r| r.id.as_deref() == Some(id)).unwrap())
    }

    #[test]
    fn disable_then_enable_round_trips_and_keeps_comments() {
        let id = "11111111-1111-1111-1111-111111111111";
        let off = set_enabled_in_yaml(RULE, id, false).unwrap();
        assert!(off.contains("enabled: false\n"));
        assert!(off.contains("# a comment that must stay"));
        assert!(!parsed_enabled(&off, id));

        // Disabling again does not add a second line.
        let off_again = set_enabled_in_yaml(&off, id, false).unwrap();
        assert_eq!(off_again.matches("enabled:").count(), 1);

        let on = set_enabled_in_yaml(&off, id, true).unwrap();
        assert_eq!(on, RULE);
        assert!(parsed_enabled(&on, id));
    }

    #[test]
    fn only_the_document_of_the_rule_changes() {
        let other = RULE
            .replace("11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222")
            .replace("Whoami", "Other");
        let text = format!("{RULE}---\n{other}");
        let off = set_enabled_in_yaml(&text, "22222222-2222-2222-2222-222222222222", false).unwrap();
        assert!(parsed_enabled(&off, "11111111-1111-1111-1111-111111111111"));
        assert!(!parsed_enabled(&off, "22222222-2222-2222-2222-222222222222"));
    }

    #[test]
    fn a_file_with_no_trailing_newline_is_edited_correctly() {
        let text = "id: abc\ntitle: T\nlogsource:\n    product: x\ndetection:\n    s:\n        a: 1\n    condition: s\n";
        // Put the id line last, with no newline after it.
        let text = format!("{}id: abc", text.replacen("id: abc\n", "", 1));
        let off = set_enabled_in_yaml(&text, "abc", false).unwrap();
        assert!(off.ends_with("id: abc\nenabled: false\n"));
        assert!(!parsed_enabled(&off, "abc"));
    }

    #[test]
    fn an_unknown_id_is_an_error() {
        assert!(set_enabled_in_yaml(RULE, "nope", false).is_err());
    }

    #[test]
    fn ids_that_could_leave_the_directory_are_rejected() {
        assert!(validate_id("11111111-1111-1111-1111-111111111111").is_ok());
        assert!(validate_id("my_rule.v2").is_ok());
        for bad in ["", "../x", "a/b", "a\\b", ".hidden", "a b"] {
            assert!(validate_id(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn write_makes_the_backup_then_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r.yaml");
        write_rule_file(&path, "one").unwrap();
        write_rule_file(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), "two");

        remove_rule_file(&path).unwrap();
        assert!(!path.exists() && !backup_path(&path).exists());
        // Removing again is not an error.
        remove_rule_file(&path).unwrap();
    }
}
