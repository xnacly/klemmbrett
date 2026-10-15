//! request:  [u32 MAGIC][u8 Command] followed by
//!           Get => [uuid], List => [u8 amount][u32 offset], Set => [uuid]
//! response: [u8 Code][u8 txtlen][text] followed on Code::Ok by
//!           Get => entry, List => [u8 len] followed by len entries, Set => nothing
//! entry:    [uuid][u8 mimelen][mime][u32 bytelen][bytes]
//!
//! integers are little endian

mod client;
mod types;

pub use client::{get, list, set};
pub use types::*;

use std::io::{self, Read};
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()))
        .join("klemmbrett.sock")
}

pub fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

pub fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub fn read_uuid(r: &mut impl Read) -> io::Result<Uuid> {
    let mut id = [0; 16];
    r.read_exact(&mut id)?;
    Ok(id)
}

fn read_bytes(r: &mut impl Read, len: usize) -> io::Result<Vec<u8>> {
    let mut b = vec![0; len];
    r.read_exact(&mut b)?;
    Ok(b)
}
