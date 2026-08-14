use async_trait::async_trait;
use nfsserve::fs_util::{metadata_to_fattr3, path_setattr};
use nfsserve::nfs::{fattr3, fileid3, filename3, nfspath3, nfsstat3, sattr3};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ROOT_ID: fileid3 = 1;

pub struct WorkspaceFs {
    root: PathBuf,
    ids: Mutex<Ids>,
}

struct Ids {
    by_id: HashMap<fileid3, PathBuf>,
    by_path: HashMap<PathBuf, fileid3>,
    next: fileid3,
}

impl WorkspaceFs {
    pub fn new(root: PathBuf) -> Self {
        let mut by_id = HashMap::new();
        let mut by_path = HashMap::new();
        by_id.insert(ROOT_ID, root.clone());
        by_path.insert(root.clone(), ROOT_ID);
        WorkspaceFs {
            root,
            ids: Mutex::new(Ids { by_id, by_path, next: ROOT_ID + 1 }),
        }
    }

    fn id_for(&self, path: PathBuf) -> fileid3 {
        let mut ids = self.ids.lock().unwrap();
        if let Some(&id) = ids.by_path.get(&path) {
            return id;
        }
        let id = ids.next;
        ids.next += 1;
        ids.by_id.insert(id, path.clone());
        ids.by_path.insert(path, id);
        id
    }

    fn path_of(&self, id: fileid3) -> Result<PathBuf, nfsstat3> {
        self.ids
            .lock()
            .unwrap()
            .by_id
            .get(&id)
            .cloned()
            .ok_or(nfsstat3::NFS3ERR_STALE)
    }

    fn forget(&self, path: &Path) {
        let mut ids = self.ids.lock().unwrap();
        if let Some(id) = ids.by_path.remove(path) {
            ids.by_id.remove(&id);
        }
    }

    async fn newnode(
        &self,
        dirid: fileid3,
        name: &filename3,
        mk: impl FnOnce(&Path) -> io::Result<()>,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let path = self.path_of(dirid)?.join(osname(name)?);
        mk(&path).map_err(nerr)?;
        let id = self.id_for(path);
        Ok((id, self.getattr(id).await?))
    }

    fn move_prefix(&self, from: &Path, to: &Path) {
        let mut ids = self.ids.lock().unwrap();
        let moved: Vec<PathBuf> = ids
            .by_path
            .keys()
            .filter(|p| p.starts_with(from))
            .cloned()
            .collect();
        for old in moved {
            let id = ids.by_path.remove(&old).unwrap();
            let new = to.join(old.strip_prefix(from).unwrap());
            ids.by_id.insert(id, new.clone());
            ids.by_path.insert(new, id);
        }
    }
}

fn nerr(e: io::Error) -> nfsstat3 {
    match e.kind() {
        io::ErrorKind::NotFound => nfsstat3::NFS3ERR_NOENT,
        io::ErrorKind::PermissionDenied => nfsstat3::NFS3ERR_ACCES,
        io::ErrorKind::AlreadyExists => nfsstat3::NFS3ERR_EXIST,
        io::ErrorKind::DirectoryNotEmpty => nfsstat3::NFS3ERR_NOTEMPTY,
        _ => nfsstat3::NFS3ERR_IO,
    }
}

fn osname(n: &filename3) -> Result<&OsStr, nfsstat3> {
    match n.as_ref() as &[u8] {
        b"" | b".." => Err(nfsstat3::NFS3ERR_ACCES),
        b if b.contains(&b'/') => Err(nfsstat3::NFS3ERR_ACCES),
        b => Ok(OsStr::from_bytes(b)),
    }
}

#[async_trait]
impl NFSFileSystem for WorkspaceFs {
    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    fn root_dir(&self) -> fileid3 {
        ROOT_ID
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let dir = self.path_of(dirid)?;
        let name: &[u8] = filename.as_ref();
        if name == b"." {
            return Ok(dirid);
        }
        if name == b".." {
            let parent = if dir == self.root {
                self.root.clone()
            } else {
                dir.parent().unwrap_or(&self.root).to_path_buf()
            };
            return Ok(self.id_for(parent));
        }
        let path = dir.join(osname(filename)?);
        path.symlink_metadata().map_err(nerr)?;
        Ok(self.id_for(path))
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        let path = self.path_of(id)?;
        let meta = path.symlink_metadata().map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                self.forget(&path);
            }
            nerr(e)
        })?;
        Ok(metadata_to_fattr3(id, &meta))
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        let path = self.path_of(id)?;
        path_setattr(&path, &setattr).await?;
        self.getattr(id).await
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let path = self.path_of(id)?;
        let f = fs::File::open(&path).map_err(nerr)?;
        let len = f.metadata().map_err(nerr)?.len();
        let mut buf = vec![0u8; count as usize];
        let mut done = 0usize;
        while done < buf.len() {
            let n = f.read_at(&mut buf[done..], offset + done as u64).map_err(nerr)?;
            if n == 0 {
                break;
            }
            done += n;
        }
        buf.truncate(done);
        Ok((buf, offset + done as u64 >= len))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let path = self.path_of(id)?;
        let f = fs::OpenOptions::new().write(true).open(&path).map_err(nerr)?;
        f.write_all_at(data, offset).map_err(nerr)?;
        self.getattr(id).await
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let (id, _) = self.newnode(dirid, filename, |p| fs::File::create(p).map(drop)).await?;
        path_setattr(&self.path_of(id)?, &attr).await?;
        Ok((id, self.getattr(id).await?))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        let mk = |p: &Path| fs::OpenOptions::new().write(true).create_new(true).open(p).map(drop);
        Ok(self.newnode(dirid, filename, mk).await?.0)
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        self.newnode(dirid, dirname, |p| fs::create_dir(p)).await
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let path = self.path_of(dirid)?.join(osname(filename)?);
        let meta = path.symlink_metadata().map_err(nerr)?;
        if meta.is_dir() {
            fs::remove_dir(&path).map_err(nerr)?;
        } else {
            fs::remove_file(&path).map_err(nerr)?;
        }
        self.forget(&path);
        Ok(())
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let from = self.path_of(from_dirid)?.join(osname(from_filename)?);
        let to = self.path_of(to_dirid)?.join(osname(to_filename)?);
        fs::rename(&from, &to).map_err(nerr)?;
        self.forget(&to);
        self.move_prefix(&from, &to);
        Ok(())
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let dir = self.path_of(dirid)?;
        let mut names: Vec<std::ffi::OsString> = fs::read_dir(&dir)
            .map_err(nerr)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        names.sort();
        let start = if start_after == 0 {
            0
        } else {
            let after = self.path_of(start_after)?;
            match after.file_name() {
                Some(name) => match names.binary_search(&name.to_os_string()) {
                    Ok(pos) => pos + 1,
                    Err(pos) => pos,
                },
                None => 0,
            }
        };
        let mut entries = Vec::new();
        let mut end = true;
        for name in names.iter().skip(start) {
            if entries.len() >= max_entries {
                end = false;
                break;
            }
            let path = dir.join(name);
            let meta = match path.symlink_metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let id = self.id_for(path);
            entries.push(DirEntry {
                fileid: id,
                name: name.as_bytes().to_vec().into(),
                attr: metadata_to_fattr3(id, &meta),
            });
        }
        Ok(ReadDirResult { entries, end })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let target = OsStr::from_bytes(symlink.as_ref());
        self.newnode(dirid, linkname, |p| std::os::unix::fs::symlink(target, p)).await
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let path = self.path_of(id)?;
        let target = fs::read_link(&path).map_err(nerr)?;
        Ok(target.into_os_string().into_vec().into())
    }
}
