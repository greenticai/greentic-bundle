use std::path::Path;

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Policy {
    Public,
    Forbidden,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmapPath {
    pub pack: Option<String>,
    pub flow: Option<String>,
    pub node: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GmapRule {
    pub path: GmapPath,
    pub policy: Policy,
    pub line: usize,
}

pub fn parse_file(path: &Path) -> anyhow::Result<Vec<GmapRule>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(path)?;
    parse_str(&contents)
}

pub fn parse_str(contents: &str) -> anyhow::Result<Vec<GmapRule>> {
    let mut rules = Vec::new();
    for (idx, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rule = parse_rule_line(line, idx + 1)?;
        rules.push(rule);
    }
    Ok(rules)
}

pub fn parse_rule_line(line: &str, line_number: usize) -> anyhow::Result<GmapRule> {
    let mut parts = line.splitn(2, '=');
    let raw_path = parts
        .next()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid rule line {line_number}: missing path"))?;
    let raw_policy = parts
        .next()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid rule line {line_number}: missing policy"))?;

    let path = parse_path(raw_path, line_number)?;
    let policy = parse_policy(raw_policy, line_number)?;
    Ok(GmapRule {
        path,
        policy,
        line: line_number,
    })
}

pub fn parse_path(raw: &str, line_number: usize) -> anyhow::Result<GmapPath> {
    if raw == "_" {
        return Ok(GmapPath {
            pack: None,
            flow: None,
            node: None,
        });
    }
    let segments: Vec<&str> = raw
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return Err(anyhow::anyhow!(
            "invalid path on line {line_number}: empty path"
        ));
    }
    if segments.len() > 3 {
        return Err(anyhow::anyhow!(
            "invalid path on line {line_number}: too many segments"
        ));
    }
    Ok(GmapPath {
        pack: Some(segments[0].to_string()),
        flow: segments.get(1).map(|segment| (*segment).to_string()),
        node: segments.get(2).map(|segment| (*segment).to_string()),
    })
}

fn parse_policy(raw: &str, line_number: usize) -> anyhow::Result<Policy> {
    match raw {
        "public" => Ok(Policy::Public),
        "forbidden" => Ok(Policy::Forbidden),
        other => Err(anyhow::anyhow!(
            "invalid policy on line {line_number}: {other}"
        )),
    }
}

impl std::fmt::Display for GmapPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.pack, &self.flow, &self.node) {
            (None, None, None) => write!(formatter, "_"),
            (Some(pack), None, None) => write!(formatter, "{pack}"),
            (Some(pack), Some(flow), None) => write!(formatter, "{pack}/{flow}"),
            (Some(pack), Some(flow), Some(node)) => write!(formatter, "{pack}/{flow}/{node}"),
            _ => write!(formatter, "_"),
        }
    }
}

impl std::fmt::Display for Policy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Policy::Public => write!(formatter, "public"),
            Policy::Forbidden => write!(formatter, "forbidden"),
        }
    }
}
