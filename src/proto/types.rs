use std::fmt;
use std::io::{self, Read, Write};

use super::{read_bytes, read_u8, read_u32, read_uuid};

pub type Uuid = [u8; 16];
pub const MAGIC: u32 = 0xDEADAFFE;

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Get = 0,
    List = 1,
    Set = 2,
}

impl TryFrom<u8> for Command {
    type Error = u8;

    fn try_from(b: u8) -> Result<Self, u8> {
        Ok(match b {
            0 => Self::Get,
            1 => Self::List,
            2 => Self::Set,
            _ => return Err(b),
        })
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Code {
    Ok = 0,
    BadMagic = 1,
    UnknownCommand = 2,
    NotFound = 3,
    Internal = 4,
}

impl TryFrom<u8> for Code {
    type Error = u8;

    fn try_from(b: u8) -> Result<Self, u8> {
        Ok(match b {
            0 => Self::Ok,
            1 => Self::BadMagic,
            2 => Self::UnknownCommand,
            3 => Self::NotFound,
            4 => Self::Internal,
            _ => return Err(b),
        })
    }
}

#[derive(Debug)]
pub struct Error {
    pub code: Code,
    pub text: String,
}

pub type Response<T> = Result<T, Error>;

impl Error {
    pub fn new(code: Code, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.text)
    }
}

impl std::error::Error for Error {}

/// Writes the `[code][txtlen][text]` head of a response
pub fn write_status(w: &mut impl Write, status: &Response<()>) -> io::Result<()> {
    let (code, text) = match status {
        Ok(()) => (Code::Ok, ""),
        Err(e) => (e.code, e.text.as_str()),
    };
    let text = &text.as_bytes()[..text.len().min(u8::MAX as usize)];
    w.write_all(&[code as u8, text.len() as u8])?;
    w.write_all(text)
}

pub fn read_status(r: &mut impl Read) -> io::Result<Response<()>> {
    let code = read_u8(r)?;
    let len = read_u8(r)?;
    let text = String::from_utf8_lossy(&read_bytes(r, len as usize)?).into_owned();
    Ok(match Code::try_from(code) {
        Ok(Code::Ok) => Ok(()),
        Ok(code) => Err(Error::new(code, text)),
        Err(code) => Err(Error::new(
            Code::Internal,
            format!("unknown response code {code}"),
        )),
    })
}

#[derive(Clone)]
pub struct Entry {
    pub id: Uuid,
    pub mime: String,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&self.id)?;
        w.write_all(&[self.mime.len() as u8])?;
        w.write_all(self.mime.as_bytes())?;
        w.write_all(&(self.data.len() as u32).to_le_bytes())?;
        w.write_all(&self.data)
    }

    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        let id = read_uuid(r)?;
        let mime_len = read_u8(r)? as usize;
        let mime = String::from_utf8_lossy(&read_bytes(r, mime_len)?).into_owned();
        let data_len = read_u32(r)? as usize;
        let data = read_bytes(r, data_len)?;
        Ok(Self { id, mime, data })
    }
}
