#![cfg(all(windows, target_arch = "x86_64"))]

mod abi;
mod config;
mod sftp;

use abi::*;
use sftp::{component, fail, join, Result, Session};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    ptr,
    sync::OnceLock,
};

static GUID: Guid = Guid(
    0x9b927e01,
    0x5271,
    0x4d39,
    [0x91, 0x80, 0xa7, 0x21, 0x57, 0x7e, 0x4c, 0x11],
);
static DISK_GUID: Guid = Guid(
    0x7b48f471,
    0x6458,
    0x4d3e,
    [0xa2, 0xd8, 0x87, 0x0c, 0xdd, 0xe0, 0x12, 0xba],
);
static UI_GUID: Guid = Guid(
    0x8e048511,
    0xda8a,
    0x4088,
    [0xab, 0xe8, 0x57, 0x95, 0xb8, 0xaf, 0x8e, 0xd4],
);
static LABEL: [u16; 4] = [83, 83, 72, 0];
static PREFIX: [u16; 4] = [115, 115, 104, 0];
struct LabelPointer(W);
// Points to the immutable static LABEL for the lifetime of the DLL.
unsafe impl Sync for LabelPointer {}
static MENU_LABEL: LabelPointer = LabelPointer(LABEL.as_ptr());

const EDIT: u64 = 8;
const FIND: u64 = 2;
const DESCR: u64 = 32;
const DIRECTORY: usize = 0x10;

#[derive(Clone, Copy)]
struct Api {
    menu: Option<Menu>,
    message: Option<Message>,
    input: Option<Input>,
    control: Option<Control>,
}
static API: OnceLock<Api> = OnceLock::new();

#[derive(PartialEq, Eq)]
struct Stamp {
    size: Option<u64>,
    modified: Option<std::time::SystemTime>,
}

struct Panel {
    alias: String,
    session: Session,
    dir: String,
    wide_dir: Vec<u16>,
    title: Vec<u16>,
    mkdir_name: Vec<u16>,
    edits: HashMap<PathBuf, (String, Stamp)>,
}

impl Panel {
    fn set_dir(&mut self, dir: String) {
        self.wide_dir = wide(&dir);
        self.title = wide(&format!("SSH {}:{}", self.alias, dir));
        self.dir = dir;
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16()
        .map(|c| if c == 0 { 0xfffd } else { c })
        .chain(Some(0))
        .collect()
}

unsafe fn read_wide(p: W) -> Result<String> {
    if p.is_null() {
        return Err(fail("Missing Far path"));
    }
    let length = (0..).find(|&n| *p.add(n) == 0).unwrap();
    Ok(String::from_utf16(std::slice::from_raw_parts(p, length))?)
}

fn message(title: &str, text: &str, flags: u64) -> isize {
    let Some(show) = API.get().and_then(|api| api.message) else {
        return -1;
    };
    let text: Vec<char> = text.replace(['\n', '\r', '\0'], " ").chars().collect();
    let mut lines = vec![wide(title)];
    lines.extend(
        text.chunks(58)
            .take(8)
            .map(|chunk| wide(&chunk.iter().collect::<String>())),
    );
    let items: Vec<_> = lines.iter().map(|line| line.as_ptr()).collect();
    unsafe {
        show(
            &GUID,
            &UI_GUID,
            flags,
            ptr::null(),
            items.as_ptr(),
            items.len(),
            0,
        )
    }
}

fn report(error: impl std::fmt::Display) {
    message("SSH", &error.to_string(), 0x10001);
}

fn input(title: &str, label: &str, initial: &str) -> Option<String> {
    let ask = API.get()?.input?;
    let (title, label, initial) = (wide(title), wide(label), wide(initial));
    let mut destination = vec![0u16; 4096];
    let accepted = unsafe {
        ask(
            &GUID,
            &UI_GUID,
            title.as_ptr(),
            label.as_ptr(),
            ptr::null(),
            initial.as_ptr(),
            destination.as_mut_ptr(),
            destination.len(),
            ptr::null(),
            0x30,
        )
    };
    if accepted == 0 {
        return None;
    }
    unsafe { read_wide(destination.as_ptr()).ok() }
}

fn choose_alias() -> Option<String> {
    let aliases = config::aliases();
    if aliases.is_empty() {
        report("No SSH aliases found in %USERPROFILE%\\.ssh\\config");
        return None;
    }
    let menu = API.get()?.menu?;
    let labels: Vec<_> = aliases.iter().map(|alias| wide(alias)).collect();
    let items: Vec<_> = labels
        .iter()
        .map(|label| MenuItem {
            flags: 0,
            text: label.as_ptr(),
            key: [0; 2],
            data: 0,
            reserved: [0; 2],
        })
        .collect();
    let index = unsafe {
        menu(
            &GUID,
            &UI_GUID,
            -1,
            -1,
            0,
            2,
            LABEL.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            items.as_ptr(),
            items.len(),
        )
    };
    aliases.get(index as usize).cloned()
}

#[no_mangle]
pub unsafe extern "system" fn GetGlobalInfoW(info: *mut Global) {
    static DESCRIPTION: OnceLock<Vec<u16>> = OnceLock::new();
    static AUTHOR: OnceLock<Vec<u16>> = OnceLock::new();
    if let Some(info) = info.as_mut() {
        info.size = std::mem::size_of::<Global>();
        info.minimum = Version(3, 0, 0, 5730, 0);
        info.version = Version(0, 1, 0, 0, 0);
        info.guid = Guid(GUID.0, GUID.1, GUID.2, GUID.3);
        info.title = LABEL.as_ptr();
        info.description = DESCRIPTION
            .get_or_init(|| wide("OpenSSH SFTP panel"))
            .as_ptr();
        info.author = AUTHOR.get_or_init(|| wide("far-ssh")).as_ptr();
    }
}

#[no_mangle]
pub unsafe extern "system" fn SetStartupInfoW(info: *const Startup) {
    if let Some(info) = info.as_ref() {
        let _ = API.set(Api {
            menu: info.menu,
            message: info.message,
            input: info.input_box,
            control: info.panel_control,
        });
    }
}

#[no_mangle]
pub unsafe extern "system" fn GetPluginInfoW(info: *mut Plugin) {
    if let Some(info) = info.as_mut() {
        info.disk = MenuDescriptor {
            guids: &DISK_GUID,
            strings: &MENU_LABEL.0,
            count: 1,
        };
        info.prefix = PREFIX.as_ptr();
    }
}

#[no_mangle]
pub unsafe extern "system" fn OpenW(info: *const Open) -> Ptr {
    let Some(info) = info.as_ref() else {
        return ptr::null_mut();
    };
    let alias = match info.from {
        0 | 10 => choose_alias(),
        4 => (info.data as *const OpenCommand)
            .as_ref()
            .and_then(|command| read_wide(command.command).ok())
            .map(|command| command.trim().to_owned())
            .and_then(|alias| {
                if alias.is_empty() {
                    choose_alias()
                } else {
                    Some(alias)
                }
            }),
        _ => None,
    };
    let Some(alias) = alias else {
        return ptr::null_mut();
    };
    if !config::valid_alias(&alias) {
        report("Invalid SSH alias");
        return ptr::null_mut();
    }
    let result = (|| {
        let session = Session::connect(&alias)?;
        let dir = session.run(sftp::directory(session.client(), "."))?;
        let mut panel = Panel {
            alias,
            session,
            dir: String::new(),
            wide_dir: Vec::new(),
            title: Vec::new(),
            mkdir_name: Vec::new(),
            edits: HashMap::new(),
        };
        panel.set_dir(dir);
        Ok::<_, Box<dyn std::error::Error>>(panel)
    })();
    match result {
        Ok(panel) => Box::into_raw(Box::new(panel)).cast(),
        Err(error) => {
            report(error);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn ClosePanelW(info: *const Close) {
    if let Some(info) = info.as_ref().filter(|info| !info.panel.is_null()) {
        drop(Box::from_raw(info.panel.cast::<Panel>()));
    }
}

#[no_mangle]
pub unsafe extern "system" fn GetOpenPanelInfoW(info: *mut PanelInfo) {
    let Some(info) = info.as_mut() else { return };
    let Some(panel) = info.panel.cast::<Panel>().as_ref() else {
        return;
    };
    info.flags = 0x0008 | 0x0040 | 0x0100; // ADDDOTS, SHOWNAMESONLY, SHOWPRESERVECASE
    info.current_dir = panel.wide_dir.as_ptr();
    info.title = panel.title.as_ptr();
    info.format = LABEL.as_ptr();
}

#[no_mangle]
pub unsafe extern "system" fn GetFindDataW(info: *mut Find) -> isize {
    let Some(info) = info.as_mut() else { return 0 };
    let Some(panel) = info.panel.cast::<Panel>().as_ref() else {
        return 0;
    };
    let entries = match panel
        .session
        .run(sftp::list(panel.session.client(), &panel.dir))
    {
        Ok(entries) => entries,
        Err(error) => {
            if info.mode & 3 == 0 {
                report(error);
            }
            return 0;
        }
    };
    let items: Box<[Item]> = entries
        .into_iter()
        .map(|entry| {
            // All fields are scalar values or nullable raw pointers.
            let mut item: Item = std::mem::zeroed();
            item.name = Box::into_raw(wide(&entry.name).into_boxed_slice()).cast();
            item.bytes = entry.size;
            item.attributes = if entry.directory { DIRECTORY } else { 0x80 };
            item
        })
        .collect();
    info.count = items.len();
    info.items = if items.is_empty() {
        ptr::null_mut()
    } else {
        Box::into_raw(items).cast()
    };
    1
}

#[no_mangle]
pub unsafe extern "system" fn FreeFindDataW(info: *const Free) {
    let Some(info) = info.as_ref().filter(|info| !info.items.is_null()) else {
        return;
    };
    let items = Box::from_raw(ptr::slice_from_raw_parts_mut(info.items, info.count));
    for item in &items {
        let len = (0..).find(|&n| *item.name.add(n) == 0).unwrap() + 1;
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            item.name.cast_mut(),
            len,
        )));
    }
}

#[no_mangle]
pub unsafe extern "system" fn SetDirectoryW(info: *const SetDirectory) -> isize {
    let Some(info) = info.as_ref() else { return 0 };
    let result = (|| {
        let panel = info
            .panel
            .cast::<Panel>()
            .as_mut()
            .ok_or_else(|| fail("Missing panel"))?;
        let dir = read_wide(info.dir)?.replace('\\', "/");
        let path = if dir.starts_with('/') {
            dir
        } else {
            join(&panel.dir, &dir)
        };
        let resolved = panel
            .session
            .run(sftp::directory(panel.session.client(), &path))?;
        panel.set_dir(resolved);
        Ok(())
    })();
    status(result, info.mode & 3 == 0, 0)
}

fn status(result: Result<()>, show: bool, error_code: isize) -> isize {
    match result {
        Ok(()) => 1,
        Err(error) => {
            if show {
                report(error);
            }
            error_code
        }
    }
}

unsafe fn names(info: &Transfer) -> Result<Vec<String>> {
    if info.count == 0 {
        return Ok(Vec::new());
    }
    if info.items.is_null() {
        return Err(fail("Missing file selection"));
    }
    std::slice::from_raw_parts(info.items, info.count)
        .iter()
        .map(|item| read_wide(item.name))
        .collect()
}

async fn stamp(client: &openssh_sftp_client::Sftp, path: &str) -> Result<Stamp> {
    let metadata = client.fs().symlink_metadata(path).await?;
    if !sftp::regular(&metadata) {
        return Err(fail("Only regular files can be edited"));
    }
    Ok(Stamp {
        size: metadata.len(),
        modified: metadata.modified().map(|time| time.as_system_time()),
    })
}

#[no_mangle]
pub unsafe extern "system" fn GetFilesW(info: *mut Transfer) -> isize {
    let Some(info) = info.as_ref() else { return -1 };
    if info.mode & DESCR != 0 {
        return -1;
    }
    let result = (|| {
        if info.move_files != 0 {
            return Err(fail("Use F5 to copy or Shift+F6 to rename"));
        }
        let panel = info
            .panel
            .cast::<Panel>()
            .as_mut()
            .ok_or_else(|| fail("Missing panel"))?;
        let dest = PathBuf::from(read_wide(info.path)?);
        let client = panel.session.client();
        let editing = info.mode & EDIT != 0;
        if editing && !client.support_posix_rename() {
            return Err(fail(
                "This SFTP server does not support atomic editor uploads",
            ));
        }
        for name in names(info)? {
            let remote = join(&panel.dir, &name);
            let local = dest.join(&name);
            let snapshot = panel.session.run(async {
                let snapshot = if editing {
                    Some(stamp(client, &remote).await?)
                } else {
                    None
                };
                sftp::download(client, remote.clone(), local.clone(), &name).await?;
                Ok(snapshot)
            })?;
            if let Some(snapshot) = snapshot {
                // Far owns the editor's temporary directory. Cap records from unchanged edits.
                panel.edits.retain(|path, _| path.is_file());
                panel.edits.insert(local, (remote, snapshot));
            }
        }
        Ok(())
    })();
    status(result, info.mode & FIND == 0, -1)
}

fn save_recovery(source: &Path) -> Result<PathBuf> {
    let mut recovery = tempfile::Builder::new()
        .prefix("far-ssh-recovery-")
        .tempfile()?;
    std::io::copy(&mut std::fs::File::open(source)?, recovery.as_file_mut())?;
    Ok(recovery.keep()?.1)
}

#[no_mangle]
pub unsafe extern "system" fn PutFilesW(info: *const Transfer) -> isize {
    let Some(info) = info.as_ref() else { return -1 };
    if info.mode & DESCR != 0 {
        return -1;
    }
    let result = (|| {
        if info.move_files != 0 {
            return Err(fail("Use F5 to copy or Shift+F6 to rename"));
        }
        let panel = info
            .panel
            .cast::<Panel>()
            .as_mut()
            .ok_or_else(|| fail("Missing panel"))?;
        let source = PathBuf::from(read_wide(info.path)?);
        let client = panel.session.client();
        for name in names(info)? {
            let path = source.join(&name);
            if info.mode & EDIT == 0 {
                panel
                    .session
                    .run(sftp::upload(client, path, &panel.dir, &name, false))?;
                continue;
            }
            let uploaded = panel.session.run(async {
                let (remote, original) = panel
                    .edits
                    .get(&path)
                    .ok_or_else(|| fail("Missing original editor file"))?;
                if &stamp(client, remote).await? != original {
                    return Err(fail(
                        "The remote file changed while it was open in the editor",
                    ));
                }
                let (dir, name) = remote
                    .rsplit_once('/')
                    .ok_or_else(|| fail("Invalid editor path"))?;
                sftp::upload(client, path.clone(), dir, name, true).await
            });
            panel.edits.remove(&path);
            if let Err(error) = uploaded {
                let recovery = match save_recovery(&path) {
                    Ok(path) => format!("Local recovery copy: {}", path.display()),
                    Err(recovery) => format!("Could not save a recovery copy: {recovery}"),
                };
                return Err(fail(format!("{error}. {recovery}")));
            }
        }
        Ok(())
    })();
    status(result, true, if info.mode & EDIT != 0 { 0 } else { -1 })
}

#[no_mangle]
pub unsafe extern "system" fn MakeDirectoryW(info: *mut MakeDirectory) -> isize {
    let Some(info) = info.as_mut() else { return -1 };
    let name = if info.mode & 1 == 0 {
        input("SSH", "New directory name", "")
    } else {
        read_wide(info.name).ok()
    };
    let Some(name) = name else { return -1 };
    let result = (|| {
        if !component(&name) {
            return Err(fail("Enter a single directory name"));
        }
        let panel = info
            .panel
            .cast::<Panel>()
            .as_mut()
            .ok_or_else(|| fail("Missing panel"))?;
        panel.session.run(async {
            panel
                .session
                .client()
                .fs()
                .create_dir(join(&panel.dir, &name))
                .await?;
            Ok(())
        })?;
        panel.mkdir_name = wide(&name);
        info.name = panel.mkdir_name.as_ptr();
        Ok(())
    })();
    status(result, true, 0)
}

#[no_mangle]
pub unsafe extern "system" fn ProcessPanelInputW(info: *const PanelInput) -> isize {
    let Some(info) = info.as_ref() else { return 0 };
    // KEY_EVENT + Shift+F6, with no Alt or Ctrl modifiers.
    if info.event_type != 1
        || info.key_down == 0
        || info.virtual_key != 0x75
        || info.modifiers & 0x1f != 0x10
    {
        return 0;
    }
    let Some(control) = API.get().and_then(|api| api.control) else {
        return 0;
    };
    let size = control(info.panel, 23, 0, ptr::null_mut());
    if size <= 0 {
        return 1;
    }
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let mut item = CurrentItem {
        size: std::mem::size_of::<CurrentItem>(),
        capacity: size as usize,
        item: buffer.as_mut_ptr().cast(),
    };
    if control(info.panel, 23, 0, (&mut item as *mut CurrentItem).cast()) == 0 {
        return 1;
    }
    let Ok(old) = read_wide((*item.item).name) else {
        return 1;
    };
    if !component(&old) {
        return 1;
    }
    let Some(new) = input("SSH rename", "New name", &old) else {
        return 1;
    };
    if old == new {
        return 1;
    }
    let result = (|| {
        if !component(&new) {
            return Err(fail("Enter a single filename"));
        }
        let panel = info
            .panel
            .cast::<Panel>()
            .as_ref()
            .ok_or_else(|| fail("Missing panel"))?;
        panel.session.run(sftp::rename(
            panel.session.client(),
            &join(&panel.dir, &old),
            &join(&panel.dir, &new),
        ))
    })();
    match result {
        Ok(()) => {
            control(info.panel, 2, 0, ptr::null_mut());
            control(info.panel, 3, 0, ptr::null_mut());
        }
        Err(error) => report(error),
    }
    1
}
