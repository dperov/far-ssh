use futures_util::TryStreamExt;
use openssh_sftp_client::{file::TokioCompatFile, Sftp};
use std::{
    error::Error,
    io,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{process::Command, runtime::Runtime, time::timeout};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn fail(message: impl Into<String>) -> Box<dyn Error> {
    io::Error::other(message.into()).into()
}

pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

pub fn component(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

// Win32 normalizes these names or treats them as devices or streams.
fn local_component(name: &str) -> Result<&str> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    let device = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if !component(name)
        || name.ends_with(['.', ' '])
        || device
        || name.chars().any(|c| c < ' ' || "<>:\"|?*".contains(c))
    {
        return Err(fail(format!("Unsupported Windows filename: {name:?}")));
    }
    Ok(name)
}

pub struct Session {
    sftp: Sftp,
    ssh: tokio::process::Child,
    runtime: Runtime,
}

impl Session {
    pub fn connect(alias: &str) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (sftp, ssh) = runtime.block_on(async {
            let mut ssh = Command::new("ssh.exe")
                .args([
                    "-T",
                    "-oBatchMode=yes",
                    "-oPreferredAuthentications=publickey",
                    "-oConnectTimeout=15",
                    "-oServerAliveInterval=15",
                    "-oServerAliveCountMax=2",
                    "-oClearAllForwardings=yes",
                    "-s",
                    alias,
                    "sftp",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .kill_on_drop(true)
                .spawn()?;
            let mut stderr = ssh.stderr.take().unwrap();
            // Drain stderr throughout the session, keeping a bounded diagnostic.
            let diagnostic = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let sink = diagnostic.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buffer = [0; 1024];
                while let Ok(count @ 1..) = stderr.read(&mut buffer).await {
                    let mut bytes = sink.lock().unwrap();
                    bytes.extend_from_slice(&buffer[..count]);
                    let excess = bytes.len().saturating_sub(4096);
                    bytes.drain(..excess);
                }
            });
            let handshake = Sftp::new(
                ssh.stdin.take().unwrap(),
                ssh.stdout.take().unwrap(),
                Default::default(),
            );
            match timeout(Duration::from_secs(25), handshake).await {
                Ok(Ok(sftp)) => Ok((sftp, ssh)),
                Ok(Err(error)) => {
                    let _ = timeout(Duration::from_millis(300), ssh.wait()).await;
                    let bytes = diagnostic.lock().unwrap();
                    Err(fail(format!(
                        "SSH {alias}: {error}. {}",
                        String::from_utf8_lossy(&bytes).trim()
                    )))
                }
                Err(_) => Err(fail(format!("SSH {alias}: connection timed out"))),
            }
        })?;
        Ok(Self { sftp, ssh, runtime })
    }

    pub fn run<T>(&self, operation: impl Future<Output = Result<T>>) -> Result<T> {
        self.runtime.block_on(operation)
    }

    pub fn client(&self) -> &Sftp {
        &self.sftp
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.ssh.start_kill();
        let _ = self
            .runtime
            .block_on(async { timeout(Duration::from_secs(1), self.ssh.wait()).await });
    }
}

use std::future::Future;

pub struct Entry {
    pub name: String,
    pub directory: bool,
    pub size: u64,
}

pub fn regular(metadata: &openssh_sftp_client::metadata::MetaData) -> bool {
    metadata.file_type().is_some_and(|kind| kind.is_file())
}

fn is_directory(metadata: &openssh_sftp_client::metadata::MetaData) -> bool {
    metadata.file_type().is_some_and(|kind| kind.is_dir())
}

pub async fn directory(sftp: &Sftp, path: &str) -> Result<String> {
    let mut fs = sftp.fs();
    let canonical = fs.canonicalize(path).await?;
    if !is_directory(&fs.metadata(&canonical).await?) {
        return Err(fail("The remote path is not a directory"));
    }
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| fail("Invalid UTF-8 remote directory"))
}

pub async fn list(sftp: &Sftp, dir: &str) -> Result<Vec<Entry>> {
    let mut stream = Box::pin(sftp.fs().open_dir(dir).await?.read_dir());
    let mut entries = Vec::new();
    while let Some(entry) = stream.try_next().await? {
        let Some(name) = entry.filename().to_str().filter(|name| component(name)) else {
            continue;
        };
        let metadata = entry.metadata();
        entries.push(Entry {
            name: name.to_owned(),
            directory: is_directory(&metadata),
            size: metadata.len().unwrap_or_default(),
        });
    }
    Ok(entries)
}

pub async fn rename(sftp: &Sftp, from: &str, to: &str) -> Result<()> {
    use openssh_sftp_client::error::{Error, SftpErrorKind};
    let mut fs = sftp.fs();
    match fs.symlink_metadata(to).await {
        Ok(_) => return Err(fail("The destination already exists")),
        Err(Error::SftpError(SftpErrorKind::NoSuchFile, _)) => {}
        Err(error) => return Err(error.into()),
    }
    fs.rename(from, to).await?;
    Ok(())
}

pub async fn download(sftp: &Sftp, remote: String, local: PathBuf, name: &str) -> Result<()> {
    local_component(name)?;
    let mut pending = vec![(remote, local, 0)];
    while let Some((source, target, depth)) = pending.pop() {
        let metadata = sftp.fs().symlink_metadata(&source).await?;
        if is_directory(&metadata) {
            if depth >= 64 {
                return Err(fail("Maximum copy depth exceeded"));
            }
            std::fs::create_dir(&target)?;
            for entry in list(sftp, &source).await? {
                pending.push((
                    join(&source, &entry.name),
                    target.join(local_component(&entry.name)?),
                    depth + 1,
                ));
            }
        } else if regular(&metadata) {
            let temp = tempfile::NamedTempFile::new_in(
                target
                    .parent()
                    .ok_or_else(|| fail("Missing destination directory"))?,
            )?;
            let mut output = tokio::fs::File::from_std(temp.reopen()?);
            let file = sftp.open(&source).await?;
            let mut input = Box::pin(TokioCompatFile::new(file.clone()));
            tokio::io::copy(&mut input, &mut output).await?;
            use tokio::io::AsyncWriteExt;
            output.flush().await?;
            drop(output);
            drop(input);
            file.close().await?;
            temp.persist_noclobber(target)?;
        } else {
            return Err(fail(format!(
                "Cannot copy links or special files: {source}"
            )));
        }
    }
    Ok(())
}

pub async fn upload(
    sftp: &Sftp,
    local: PathBuf,
    dir: &str,
    name: &str,
    replace: bool,
) -> Result<()> {
    if !component(name) {
        return Err(fail("Invalid remote filename"));
    }
    let mut pending = vec![(local, dir.to_owned(), name.to_owned(), 0)];
    while let Some((source, parent, name, depth)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&source)?;
        let target = join(&parent, &name);
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(fail(format!(
                "Cannot copy reparse point: {}",
                source.display()
            )));
        }
        if metadata.is_dir() && !replace {
            if depth >= 64 {
                return Err(fail("Maximum copy depth exceeded"));
            }
            sftp.fs().create_dir(&target).await?;
            for entry in std::fs::read_dir(source)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| fail("Invalid Unicode filename"))?;
                pending.push((entry.path(), target.clone(), name, depth + 1));
            }
        } else if metadata.is_file() {
            upload_file(sftp, &source, &parent, &target, replace).await?;
        } else {
            return Err(fail(format!(
                "Cannot copy special file: {}",
                source.display()
            )));
        }
    }
    Ok(())
}

async fn upload_file(
    sftp: &Sftp,
    source: &Path,
    parent: &str,
    target: &str,
    replace: bool,
) -> Result<()> {
    let mut input = tokio::fs::File::open(source).await?;
    if !replace {
        // Exclusive creation prevents overwriting a file created concurrently.
        let file = sftp
            .options()
            .write(true)
            .create_new(true)
            .open(target)
            .await?;
        let result = write_file(file, &mut input).await;
        if result.is_err() {
            let _ = sftp.fs().remove_file(target).await;
        }
        return result;
    }
    if !sftp.support_posix_rename() || !regular(&sftp.fs().symlink_metadata(target).await?) {
        return Err(fail(
            "Editing requires an existing regular file and atomic rename support",
        ));
    }
    let temp = join(parent, &format!(".far-ssh-{}.tmp", uuid::Uuid::new_v4()));
    let mode = sftp
        .fs()
        .metadata(target)
        .await?
        .permissions()
        .ok_or_else(|| fail("Missing remote permissions"))?;
    let file = sftp
        .options()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await?;
    let result = async {
        write_file(file, &mut input).await?;
        let attrs = openssh_sftp_client::metadata::MetaDataBuilder::new()
            .permissions(mode)
            .create();
        sftp.fs().set_metadata(&temp, attrs).await?;
        sftp.fs().rename(&temp, target).await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = sftp.fs().remove_file(&temp).await;
    }
    result
}

async fn write_file(
    file: openssh_sftp_client::file::File,
    input: &mut tokio::fs::File,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut output = Box::pin(TokioCompatFile::new(file.clone()));
    let result = async {
        tokio::io::copy(input, &mut output).await?;
        output.flush().await
    }
    .await;
    drop(output);
    let close = file.close().await;
    result?;
    close?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires OpenSSH sftp-server.exe in PATH"]
    fn local_sftp_round_trip() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let destination = tempfile::tempdir()?;
        let tree = source.path().join("tree");
        std::fs::create_dir_all(tree.join("nested"))?;
        let file = tree.join("nested").join("тест [1].txt");
        let data = vec![0x8c; 100_017];
        std::fs::write(&file, &data)?;

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let mut child = Command::new("sftp-server.exe")
                    .current_dir(root.path())
                    .kill_on_drop(true)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?;
                let client = Sftp::new(
                    child.stdin.take().unwrap(),
                    child.stdout.take().unwrap(),
                    Default::default(),
                )
                .await?;
                let home = directory(&client, ".").await?;
                client.fs().create_dir(join(&home, "incoming")).await?;
                let incoming = join(&home, "incoming");
                upload(&client, tree, &incoming, "tree", false).await?;
                let original = join(&incoming, "tree");
                let renamed = join(&incoming, "renamed");
                rename(&client, &original, &renamed).await?;
                assert!(rename(&client, &renamed, &renamed).await.is_err());
                let output = destination.path().join("renamed");
                download(&client, renamed.clone(), output.clone(), "renamed").await?;
                assert_eq!(
                    std::fs::read(output.join("nested").join("тест [1].txt"))?,
                    data
                );

                std::fs::write(&file, b"saved from editor")?;
                let nested = join(&renamed, "nested");
                assert!(
                    upload(&client, file.clone(), &nested, "тест [1].txt", false)
                        .await
                        .is_err()
                );
                upload(&client, file, &nested, "тест [1].txt", true).await?;
                assert_eq!(
                    &client.fs().read(join(&nested, "тест [1].txt")).await?[..],
                    b"saved from editor"
                );
                assert_eq!(list(&client, &nested).await?.len(), 1);
                client.close().await?;
                Ok(())
            })
    }
}
