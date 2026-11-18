//! Far Manager 3 Unicode API, Windows x64. Layouts from FarGroup/FarManager far/plugin.hpp.
use std::ffi::c_void;
pub type Ptr = *mut c_void;
pub type W = *const u16;

#[repr(C)]
pub struct Guid(pub u32, pub u16, pub u16, pub [u8; 8]);
#[repr(C)]
pub struct Version(pub u32, pub u32, pub u32, pub u32, pub i32);
#[repr(C)]
pub struct Global {
    pub size: usize,
    pub minimum: Version,
    pub version: Version,
    pub guid: Guid,
    pub title: W,
    pub description: W,
    pub author: W,
    pub instance: Ptr,
}
#[repr(C)]
pub struct MenuDescriptor {
    pub guids: *const Guid,
    pub strings: *const W,
    pub count: usize,
}
#[repr(C)]
pub struct Plugin {
    pub size: usize,
    pub flags: u64,
    pub disk: MenuDescriptor,
    pub plugins: MenuDescriptor,
    pub config: MenuDescriptor,
    pub prefix: W,
    pub instance: Ptr,
}
#[repr(C)]
pub struct MenuItem {
    pub flags: u64,
    pub text: W,
    pub key: [u32; 2],
    pub data: isize,
    pub reserved: [isize; 2],
}

pub type Menu = unsafe extern "system" fn(
    *const Guid,
    *const Guid,
    isize,
    isize,
    isize,
    u64,
    W,
    W,
    W,
    Ptr,
    Ptr,
    *const MenuItem,
    usize,
) -> isize;
pub type Message =
    unsafe extern "system" fn(*const Guid, *const Guid, u64, W, *const W, usize, isize) -> isize;
pub type Input = unsafe extern "system" fn(
    *const Guid,
    *const Guid,
    W,
    W,
    W,
    W,
    *mut u16,
    usize,
    W,
    u64,
) -> isize;
pub type Control = unsafe extern "system" fn(Ptr, i32, isize, Ptr) -> isize;

#[repr(C)]
pub struct Startup {
    pub size: usize,
    pub module_name: W,
    pub menu: Option<Menu>,
    pub message: Option<Message>,
    pub get_msg: usize,
    pub panel_control: Option<Control>,
    pub intervening_callbacks: [usize; 13],
    pub input_box: Option<Input>,
}
#[repr(C)]
pub struct Open {
    pub size: usize,
    pub from: i32,
    pub guid: *const Guid,
    pub data: isize,
    pub instance: Ptr,
}
#[repr(C)]
pub struct OpenCommand {
    pub size: usize,
    pub command: W,
}
#[repr(C)]
pub struct Close {
    pub size: usize,
    pub panel: Ptr,
    pub instance: Ptr,
}
#[repr(C)]
pub struct PanelInfo {
    pub size: usize,
    pub panel: Ptr,
    pub flags: u64,
    pub host_file: W,
    pub current_dir: W,
    pub format: W,
    pub title: W,
    pub info_lines: Ptr,
    pub info_count: usize,
    pub description_files: Ptr,
    pub description_count: usize,
    pub modes: Ptr,
    pub mode_count: usize,
    pub start_mode: isize,
    pub sort_mode: i32,
    pub sort_order: isize,
    pub key_bar: Ptr,
    pub shortcut: W,
    pub free_size: u64,
    pub user_data: [usize; 2],
    pub instance: Ptr,
}
#[repr(C)]
pub struct Item {
    pub times: [[u32; 2]; 4],
    pub bytes: u64,
    pub allocation: u64,
    pub name: W,
    pub alternate: W,
    pub description: W,
    pub owner: W,
    pub columns: Ptr,
    pub column_count: usize,
    pub flags: u64,
    pub user_data: [usize; 2],
    pub attributes: usize,
    pub links: usize,
    pub crc: usize,
    pub reserved: [isize; 2],
}
#[repr(C)]
pub struct Find {
    pub size: usize,
    pub panel: Ptr,
    pub items: *mut Item,
    pub count: usize,
    pub mode: u64,
    pub instance: Ptr,
}
#[repr(C)]
pub struct Free {
    pub size: usize,
    pub panel: Ptr,
    pub items: *mut Item,
    pub count: usize,
    pub instance: Ptr,
}
#[repr(C)]
pub struct SetDirectory {
    pub size: usize,
    pub panel: Ptr,
    pub dir: W,
    pub reserved: isize,
    pub mode: u64,
    pub user_data: [usize; 2],
    pub instance: Ptr,
}
#[repr(C)]
pub struct Transfer {
    pub size: usize,
    pub panel: Ptr,
    pub items: *mut Item,
    pub count: usize,
    pub move_files: i32,
    pub path: W,
    pub mode: u64,
    pub instance: Ptr,
}
#[repr(C)]
pub struct MakeDirectory {
    pub size: usize,
    pub panel: Ptr,
    pub name: W,
    pub mode: u64,
    pub instance: Ptr,
}
#[repr(C)]
pub struct PanelInput {
    pub size: usize,
    pub panel: Ptr,
    pub event_type: u16,
    pub padding: u16,
    pub key_down: i32,
    pub repeat: u16,
    pub virtual_key: u16,
    pub scan_code: u16,
    pub character: u16,
    pub modifiers: u32,
    pub instance: Ptr,
}
#[repr(C)]
pub struct CurrentItem {
    pub size: usize,
    pub capacity: usize,
    pub item: *mut Item,
}
