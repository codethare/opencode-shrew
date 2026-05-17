use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::models::{AutoTagRule, Session};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OcsMeta {
    #[serde(default)]
    pub tags: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub notes: HashMap<String, String>,
    #[serde(default)]
    pub autotag_rules: Vec<AutoTagRule>,
}



fn meta_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/oc".to_string());
    let mut path = PathBuf::from(home);
    path.push(".local/share/opencode/ocs_meta.json");
    path
}

pub fn load_meta() -> Result<OcsMeta> {
    let path = meta_path();
    if !path.exists() {
        return Ok(OcsMeta {
            tags: HashMap::new(),
            notes: HashMap::new(),
            autotag_rules: Vec::new(),
        });
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read meta file: {}", path.display()))?;
    let meta: OcsMeta = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse meta file: {}", path.display()))?;
    Ok(meta)
}

pub fn save_meta(meta: &OcsMeta) -> Result<()> {
    let path = meta_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(meta)?;
    // Atomic write via O_EXCL temp + rename — prevents symlink races (TOCTOU)
    let tmp_path = path.with_extension("json.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&tmp_path)
        .with_context(|| format!("Failed to create meta temp file: {}", tmp_path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write meta temp file: {}", tmp_path.display()))?;
    file.flush()?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("Failed to rename meta file: {}", path.display()))?;
    Ok(())
}

pub fn add_tag(session_id: &str, tag: &str) -> Result<()> {
    let mut meta = load_meta()?;
    let tags = meta.tags.entry(session_id.to_string()).or_default();
    if !tags.contains(&tag.to_string()) {
        tags.push(tag.to_string());
        tags.sort();
        tags.dedup();
    }
    save_meta(&meta)?;
    Ok(())
}

pub fn remove_tag(session_id: &str, tag: &str) -> Result<bool> {
    let mut meta = load_meta()?;
    let should_remove = meta.tags.get(session_id)
        .map_or(false, |tags| tags.iter().any(|t| t == tag));
    if !should_remove {
        return Ok(false);
    }
    if let Some(tags) = meta.tags.get_mut(session_id) {
        tags.retain(|t| t != tag);
    }
    if meta.tags.get(session_id).map_or(false, |tags| tags.is_empty()) {
        meta.tags.remove(session_id);
    }
    save_meta(&meta)?;
    Ok(true)
}

pub fn list_tags(session_id: &str) -> Result<Vec<String>> {
    let meta = load_meta()?;
    Ok(meta.tags.get(session_id).cloned().unwrap_or_default())
}

pub fn search_by_tag(tag: &str) -> Result<Vec<(String, Vec<String>)>> {
    let meta = load_meta()?;
    let mut results: Vec<(String, Vec<String>)> = meta
        .tags
        .iter()
        .filter(|(_, tags)| tags.iter().any(|t| t == tag))
        .map(|(id, tags)| (id.clone(), tags.clone()))
        .collect();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

pub fn set_note(session_id: &str, text: &str) -> Result<()> {
    let mut meta = load_meta()?;
    if text.is_empty() {
        meta.notes.remove(session_id);
    } else {
        meta.notes.insert(session_id.to_string(), text.to_string());
    }
    save_meta(&meta)
}

pub fn get_note(session_id: &str) -> Result<Option<String>> {
    let meta = load_meta()?;
    Ok(meta.notes.get(session_id).cloned())
}

pub fn remove_note(session_id: &str) -> Result<bool> {
    let mut meta = load_meta()?;
    let existed = meta.notes.contains_key(session_id);
    meta.notes.remove(session_id);
    save_meta(&meta)?;
    Ok(existed)
}

pub fn list_annotated_ids() -> Result<Vec<String>> {
    let meta = load_meta()?;
    let mut ids: Vec<String> = meta.notes.keys().cloned().collect();
    ids.sort();
    Ok(ids)
}

/// Check if a session matches a single autotag rule
pub fn rule_matches(session: &Session, rule: &AutoTagRule) -> bool {
    if let Some(ref kw) = rule.title_contains {
        if !session.title.to_lowercase().contains(&kw.to_lowercase()) {
            return false;
        }
    }
    if let Some(ref dir) = rule.dir_contains {
        if !session.directory.to_lowercase().contains(&dir.to_lowercase()) {
            return false;
        }
    }
    if let Some(ref m) = rule.model {
        if session.model.as_deref().map_or(true, |model| {
            !model.to_lowercase().contains(&m.to_lowercase())
        }) {
            return false;
        }
    }
    if let Some(min) = rule.min_cost {
        if session.total_cost < min {
            return false;
        }
    }
    if let Some(max) = rule.max_cost {
        if session.total_cost > max {
            return false;
        }
    }
    if let Some(min) = rule.min_messages {
        if session.msg_count < min {
            return false;
        }
    }
    if let Some(max) = rule.max_messages {
        if session.msg_count > max {
            return false;
        }
    }
    true
}

// ── Autotag rule management ──────────────────────────────────────────

pub fn list_autotag_rules() -> Result<Vec<AutoTagRule>> {
    let meta = load_meta()?;
    Ok(meta.autotag_rules)
}

pub fn add_autotag_rule(rule: AutoTagRule) -> Result<()> {
    let mut meta = load_meta()?;
    meta.autotag_rules.push(rule);
    save_meta(&meta)
}

pub fn remove_autotag_rule(rule_id: &str) -> Result<bool> {
    let mut meta = load_meta()?;
    let len_before = meta.autotag_rules.len();
    meta.autotag_rules.retain(|r| r.id != rule_id);
    let removed = meta.autotag_rules.len() < len_before;
    if removed {
        save_meta(&meta)?;
    }
    Ok(removed)
}

/// Apply autotag rules to a session. Returns the list of tags that were added.
pub fn apply_rules_to_session(session: &Session) -> Result<Vec<String>> {
    let mut meta = load_meta()?;
    let mut added = Vec::new();
    for rule in &meta.autotag_rules {
        if rule_matches(session, rule) {
            let tags = meta.tags.entry(session.id.clone()).or_default();
            if !tags.contains(&rule.tag) {
                tags.push(rule.tag.clone());
                added.push(rule.tag.clone());
            }
        }
    }
    if !added.is_empty() {
        save_meta(&meta)?;
    }
    Ok(added)
}

pub fn all_tags() -> Result<Vec<(String, usize)>> {
    let meta = load_meta()?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tags) in &meta.tags {
        for tag in tags {
            *counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }
    let mut result: Vec<(String, usize)> = counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(result)
}
