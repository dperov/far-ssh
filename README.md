# far-ssh

An SFTP panel plugin for Far Manager 3 x64 (build 5730 or later), written
in Rust. It connects using your existing OpenSSH host aliases.

## Usage

Press **Alt+F1** or **Alt+F2**, choose **SSH**, and select an alias. The
panel opens in the remote home directory. You can also enter `ssh:` in
Far's command line to show the alias list, or `ssh:my-host` to connect directly.

The plugin lists explicit `Host` aliases from `%USERPROFILE%\.ssh\config`
and files referenced by `Include`. OpenSSH applies wildcard entries such
as `Host *` during connection, along with settings such as `HostName`,
`User`, `IdentityFile`, and `ProxyJump`.

The connection requires `ssh.exe` on `PATH`, an available private key
or `ssh-agent`, and a server host key trusted by OpenSSH. The plugin uses
non-interactive public-key authentication.

- **F5** copies selected files and directories between panels, including
  nested contents. A name collision stops the operation. Completed copies
  and directories already created remain in place. Symbolic links and
  special files produce an error.
- **F7** creates a directory in the current remote directory.
- **Shift+F6** renames the item under the cursor in the current directory.
- **F4** opens a remote file in Far's editor. Save and close the editor
  to upload the changes to the original path. The server must support
  `posix-rename@openssh.com` for atomic replacement. The plugin checks the
  remote file's size and modification time before uploading. If the upload
  fails, it attempts to keep a local recovery copy.

## Build and install

From a Windows x64 Rust environment:

```powershell
cargo +1.95.0 build --release --locked
```

Copy `target\release\far_ssh.dll` into a dedicated `Plugins\FarSSH`
folder under your Far Manager installation and restart Far.

## Test

With OpenSSH's `sftp-server.exe` on `PATH`, run:

```powershell
cargo +1.95.0 test --locked -- --include-ignored
```

The integration test uses a local SFTP subprocess and temporary directories.
