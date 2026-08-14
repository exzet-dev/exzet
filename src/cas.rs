use crate::proto::{clean_rel, FileEntry};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

pub fn valid_hash(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
}

pub struct Scan {
    pub files: Vec<FileEntry>,
    pub by_hash: HashMap<String, PathBuf>,
}

pub fn scan(root: &Path) -> Result<Scan> {
    let mut files = Vec::new();
    let mut by_hash = HashMap::new();
    let walker = ignore::WalkBuilder::new(root).hidden(false).git_ignore(true).git_global(false)
        .git_exclude(true).require_git(false).filter_entry(|e| e.file_name() != ".git").build();
    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if path == root {
            continue;
        }
        let meta = path.symlink_metadata()?;
        let rel = path.strip_prefix(root)?.to_string_lossy().into_owned();
        if meta.is_symlink() {
            let target = fs::read_link(path)?.to_string_lossy().into_owned();
            files.push(FileEntry {
                path: rel,
                mode: 0o777,
                hash: String::new(),
                link: Some(target),
            });
        } else if meta.is_file() {
            let hash = hash_file(path)?;
            by_hash.insert(hash.clone(), path.to_path_buf());
            files.push(FileEntry {
                path: rel,
                mode: meta.permissions().mode() & 0o777,
                hash,
                link: None,
            });
        }
    }
    Ok(Scan { files, by_hash })
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store> {
        fs::create_dir_all(dir)?;
        Ok(Store { root: dir.to_path_buf() })
    }

    pub fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }

    pub fn missing(&self, files: &[FileEntry]) -> Vec<String> {
        let mut v: Vec<String> = files
            .iter()
            .filter(|f| f.link.is_none() && !self.blob_path(&f.hash).is_file())
            .map(|f| f.hash.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn insert(&self, hash: &str, tmp: &Path) -> Result<()> {
        if hash_file(tmp)? != hash {
            let _ = fs::remove_file(tmp);
            bail!("blob does not match its hash");
        }
        let dest = self.blob_path(hash);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::set_permissions(tmp, fs::Permissions::from_mode(0o444))?;
        fs::rename(tmp, &dest)?;
        Ok(())
    }

    pub fn hydrate(&self, files: &[FileEntry], dest: &Path) -> Result<()> {
        for f in files {
            let target = dest.join(clean_rel(&f.path)?);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            match &f.link {
                Some(t) => symlink(t, &target)
                    .with_context(|| format!("linking {}", f.path))?,
                None => {
                    fs::copy(self.blob_path(&f.hash), &target)
                        .with_context(|| format!("hydrating {}", f.path))?;
                    fs::set_permissions(&target, fs::Permissions::from_mode(f.mode | 0o600))?;
                }
            }
        }
        Ok(())
    }
}
