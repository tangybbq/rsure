// Playing with paths.

use std::ffi::{CString};
use std::io;
use std::ptr;

extern crate libc;

fn main() {
    unsafe { blop() };
    stuff().unwrap();
}

fn stuff() -> io::Result<()> {
    let path = b"/usr/bin".to_vec();
    let p = CString::new(path).unwrap();
    let len = unsafe { libc::pathconf(p.as_ptr() as *mut _, libc::_PC_NAME_MAX) as usize };
    let name_offset = unsafe { dir_name_offset() as usize };
    println!("len: {}", len);
    println!("offset: {}", name_offset);

    unsafe {
        let dirp = libc::opendir(p.as_ptr());
        if dirp.is_null() {
            return Err(io::Error::last_os_error());
        }
        println!("dir: {:?}", dirp);

        let mut buf: Vec<u8> = Vec::with_capacity(name_offset + len + 1);
        let ptr = buf.as_mut_ptr() as *mut libc::dirent_t;
        let mut entry_ptr = ptr::null_mut();
        if libc::readdir_r(dirp, ptr, &mut entry_ptr) != 0 {
            // Close dir?
            return Err(io::Error::last_os_error());
        }
        if entry_ptr.is_null() {
            println!("No more entries");
        }
    }
    Ok(())
}

extern {
    fn dir_name_offset() -> libc::size_t;
    fn blop();
}
