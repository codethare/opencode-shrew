use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::models::AutoTagRule;

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
    let _ = std::fs::remove_file(&tmp_path);
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
