use std::io::{self, BufReader, Write};
use std::os::unix::net::UnixStream;

use super::{Command, Entry, MAGIC, Uuid, read_status, read_u8, socket_path};

fn send(cmd: Command, payload: &[u8]) -> io::Result<BufReader<UnixStream>> {
    let mut stream = UnixStream::connect(socket_path())?;
    let mut req = MAGIC.to_le_bytes().to_vec();
    req.push(cmd as u8);
    req.extend_from_slice(payload);
    stream.write_all(&req)?;

    let mut r = BufReader::new(stream);
    read_status(&mut r)?.map_err(io::Error::other)?;
    Ok(r)
}

pub fn get(id: Uuid) -> io::Result<Entry> {
    Entry::read(&mut send(Command::Get, &id)?)
}

pub fn set(id: Uuid) -> io::Result<()> {
    send(Command::Set, &id).map(drop)
}

pub fn list(amount: u8) -> io::Result<Vec<Entry>> {
    let mut r = send(Command::List, &[amount])?;
    let len = read_u8(&mut r)?;
    (0..len).map(|_| Entry::read(&mut r)).collect()
}
