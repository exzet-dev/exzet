use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "exfile";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub name: String,
    pub script: String,
    #[serde(skip)]
    pub servers: Vec<String>,
    #[serde(skip)]
    pub deps: Vec<String>,
    pub env: Vec<(String, String)>,
    pub shell: String,
    pub args: Vec<String>,
    pub image: Option<String>,
    pub cpus: Option<u32>,
    pub mem_bytes: Option<u64>,
    pub gpus: u32,
    pub disk_bytes: Option<u64>,
    pub time_secs: Option<u64>,
    pub replicas: u32,
    pub live: bool,
    pub outputs: Vec<String>,
}

pub fn resolve(start: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut dir = start.to_path_buf();
    loop {
        let plain = dir.join(FILE_NAME);
        if plain.is_file() {
            return Ok((dir, plain));
        }
        let mut named: Vec<String> = std::fs::read_dir(&dir)
            .map(|it| {
                it.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.ends_with(".exfile") && n.len() > ".exfile".len())
                    .collect()
            })
            .unwrap_or_default();
        named.sort();
        match named.len() {
            0 => {}
            1 => return Ok((dir.clone(), dir.join(&named[0]))),
            _ => bail!("{}: {} (pick with -f)", dir.display(), named.join(" ")),
        }
        if !dir.pop() {
            bail!("no {FILE_NAME} found from {} upward", start.display());
        }
    }
}

pub fn dotenv(dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(dir.join(".env"))
        .map(|t| {
            t.lines()
                .map(str::trim)
                .map(|l| l.strip_prefix("export ").unwrap_or(l))
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), unquote(v))))
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse(text: &str, dotenv: &[(String, String)]) -> Result<Vec<Task>> {
    let mut tasks: Vec<Task> = Vec::new();
    let mut defaults = Task {
        replicas: 1,
        env: dotenv.to_vec(),
        ..Task::default()
    };
    let mut attrs: Vec<(String, String, usize)> = Vec::new();
    let mut cur: Option<(Task, Vec<String>, usize)> = None;

    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim_end();
        let indented = raw.starts_with(' ') || raw.starts_with('\t');

        if let Some((_, lines, _)) = cur.as_mut() {
            if indented || line.trim().is_empty() {
                lines.push(line.to_string());
                continue;
            }
        }
        if let Some((t, lines, hn)) = cur.take() {
            finish_task(&mut tasks, t, &lines, hn)?;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(inner) = trimmed.strip_prefix('[') {
            let inner = inner
                .strip_suffix(']')
                .with_context(|| format!("line {n}: expected [key: value, ...]"))?;
            for part in inner.split(',') {
                let (k, v) = part
                    .split_once(':')
                    .with_context(|| format!("line {n}: expected key: value inside [ ]"))?;
                let v = expand(&unquote(v), &defaults.env, n)?;
                attrs.push((k.trim().to_string(), v, n));
            }
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(":=") {
            let (k, v) = (k.trim(), expand(&unquote(v), &defaults.env, n)?);
            if env_key(k) {
                defaults.env.push((k.to_string(), v));
            } else {
                set(&mut defaults, k, &v, n)?;
            }
            continue;
        }
        if let Some((name, deps)) = trimmed.split_once(':') {
            let name = name.trim();
            if !valid_name(name) {
                bail!("line {n}: invalid task name '{name}'");
            }
            let mut t = defaults.clone();
            t.name = name.to_string();
            for d in deps.split_whitespace() {
                if !valid_name(d) {
                    bail!("line {n}: invalid dependency '{d}'");
                }
                t.deps.push(d.to_string());
            }
            for (k, v, an) in attrs.drain(..) {
                if env_key(&k) {
                    t.env.push((k, v));
                } else {
                    set(&mut t, &k, &v, an)?;
                }
            }
            cur = Some((t, Vec::new(), n));
            continue;
        }
        bail!("line {n}: unrecognized line '{trimmed}'");
    }
    if let Some((t, lines, hn)) = cur.take() {
        finish_task(&mut tasks, t, &lines, hn)?;
    }
    if let Some((_, _, n)) = attrs.first() {
        bail!("line {n}: attributes without a following task");
    }
    if tasks.is_empty() {
        bail!("no tasks defined");
    }
    for t in &tasks {
        schedule(&tasks, &t.name)?;
    }
    Ok(tasks)
}

pub fn schedule<'a>(tasks: &'a [Task], name: &str) -> Result<Vec<&'a Task>> {
    fn visit<'a>(tasks: &'a [Task], name: &str, path: &mut Vec<String>, out: &mut Vec<&'a Task>) -> Result<()> {
        if out.iter().any(|t| t.name == name) {
            return Ok(());
        }
        if path.iter().any(|p| p == name) {
            bail!("dependency cycle at '{name}'");
        }
        let t = tasks.iter().find(|t| t.name == name).with_context(|| match path.last() {
            Some(p) => format!("task '{p}' depends on unknown task '{name}'"),
            None => format!("no task '{name}'"),
        })?;
        path.push(name.to_string());
        for d in &t.deps {
            visit(tasks, d, path, out)?;
        }
        path.pop();
        out.push(t);
        Ok(())
    }
    let mut out = Vec::new();
    visit(tasks, name, &mut Vec::new(), &mut out)?;
    Ok(out)
}

fn finish_task(tasks: &mut Vec<Task>, mut t: Task, lines: &[String], hn: usize) -> Result<()> {
    t.script = dedent(lines);
    if t.script.is_empty() && t.deps.is_empty() {
        bail!("task '{}' (line {hn}) has an empty body", t.name);
    }
    if t.live && t.replicas > 1 {
        bail!("task '{}': live workspace requires replicas = 1", t.name);
    }
    if t.gpus > 0 && t.image.is_none() {
        bail!("task '{}': gpus requires an image", t.name);
    }
    if tasks.iter().any(|e| e.name == t.name) {
        bail!("task '{}' defined twice", t.name);
    }
    tasks.push(t);
    Ok(())
}

fn expand(v: &str, vars: &[(String, String)], n: usize) -> Result<String> {
    let mut out = String::new();
    let mut rest = v;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('}').with_context(|| format!("line {n}: unclosed ${{"))? + i;
        let name = &rest[i + 2..end];
        let val = vars
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .with_context(|| format!("line {n}: undefined variable '{name}'"))?;
        out.push_str(val);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn env_key(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn set(t: &mut Task, k: &str, v: &str, n: usize) -> Result<()> {
    match k {
        "servers" => t.servers = v.split_whitespace().map(str::to_string).collect(),
        "shell" => {
            if v.trim().is_empty() {
                bail!("line {n}: shell needs a command");
            }
            t.shell = v.to_string();
        }
        "image" => t.image = Some(v.to_string()),
        "cpus" => t.cpus = Some(num(v, n)?),
        "mem" => t.mem_bytes = Some(size(v, n)?),
        "gpus" => t.gpus = num(v, n)?,
        "disk" => t.disk_bytes = Some(size(v, n)?),
        "time" => t.time_secs = Some(dur(v, n)?),
        "replicas" => t.replicas = num(v, n)?.max(1),
        "workspace" => {
            t.live = match v {
                "live" => true,
                "sync" => false,
                _ => bail!("line {n}: workspace must be 'sync' or 'live'"),
            }
        }
        "outputs" => {
            t.outputs = v
                .split_whitespace()
                .map(|s| s.trim_start_matches("./").trim_matches('/').to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => bail!("line {n}: unknown key '{k}' (UPPERCASE keys define env vars)"),
    }
    Ok(())
}

fn num(v: &str, n: usize) -> Result<u32> {
    v.parse().with_context(|| format!("line {n}: expected a number, got '{v}'"))
}

fn size(v: &str, n: usize) -> Result<u64> {
    scaled(v, n, &[('k', 1 << 10), ('m', 1 << 20), ('g', 1 << 30), ('t', 1 << 40)], "64g")
}

fn dur(v: &str, n: usize) -> Result<u64> {
    scaled(v, n, &[('s', 1), ('m', 60), ('h', 3600), ('d', 86400)], "12h")
}

fn scaled(v: &str, n: usize, units: &[(char, u64)], like: &str) -> Result<u64> {
    let s = v.trim().to_ascii_lowercase();
    let (digits, mult) = match s.chars().last().and_then(|c| units.iter().find(|(u, _)| *u == c)) {
        Some(&(_, m)) => (&s[..s.len() - 1], m),
        None => (s.as_str(), 1),
    };
    let base: u64 = digits
        .parse()
        .with_context(|| format!("line {n}: expected a value like {like}, got '{v}'"))?;
    Ok(base * mult)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !matches!(s, "ps" | "attach")
}

fn dedent(lines: &[String]) -> String {
    let prefix = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| if l.len() >= prefix { &l[prefix..] } else { "" })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}
