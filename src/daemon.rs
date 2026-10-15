use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::proto::{self, Code, Command, Entry, Response, Uuid};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self as device, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self as offer, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self as source, ZwlrDataControlSourceV1},
};

/// mime for things klemmbrett wrote to the clipboard, so selecting an entry doesn't duplicate it in
/// the history by storing it in our history
const MARKER_MIME: &str = "application/x-klemmbrett";
const MAX_ENTRIES: usize = 100;

/// Dispatch state of the wayland thread, cloned into the socket thread
#[derive(Clone)]
struct Daemon {
    conn: Connection,
    qh: QueueHandle<Daemon>,
    manager: ZwlrDataControlManagerV1,
    device: ZwlrDataControlDeviceV1,
    /// newest entry first
    history: Arc<Mutex<VecDeque<Entry>>>,
}

impl Daemon {
    fn history(&self) -> MutexGuard<'_, VecDeque<Entry>> {
        self.history.lock().unwrap()
    }

    /// Moves entry `id` to the front and makes it the clipboard content
    fn set(&self, id: Uuid) -> Response<()> {
        let mut history = self.history();
        let idx = history.iter().position(|e| e.id == id).ok_or_else(not_found)?;
        let entry = history.remove(idx).unwrap();
        history.push_front(entry.clone());

        let source = self.manager.create_data_source(&self.qh, entry.clone());
        source.offer(entry.mime);
        source.offer(MARKER_MIME.into());
        self.device.set_selection(Some(&source));
        self.conn.flush().map_err(|e| proto::Error::new(Code::Internal, e.to_string()))
    }
}

pub fn run() -> Result<(), Box<dyn Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Daemon>(&conn)?;
    let qh = queue.handle();
    let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
    let manager: ZwlrDataControlManagerV1 = globals.bind(&qh, 1..=1, ())?;
    let device = manager.get_data_device(&seat, &qh, ());
    let mut daemon = Daemon { conn, qh, manager, device, history: Default::default() };

    let path = proto::socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    let d = daemon.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = handle_client(stream, &d);
        }
    });

    loop {
        queue.blocking_dispatch(&mut daemon)?;
    }
}

fn not_found() -> proto::Error {
    proto::Error::new(Code::NotFound, "no entry with this uuid")
}

fn io_err(e: io::Error) -> proto::Error {
    proto::Error::new(Code::Internal, e.to_string())
}

fn handle_client(mut stream: UnixStream, daemon: &Daemon) -> io::Result<()> {
    let mut payload = Vec::new();
    let status = respond(&mut stream, &mut payload, daemon);
    proto::write_status(&mut stream, &status)?;
    if status.is_ok() {
        stream.write_all(&payload)?;
    }
    Ok(())
}

/// Reads a request and writes the payload of its response to `out`
fn respond(r: &mut impl Read, out: &mut Vec<u8>, daemon: &Daemon) -> Response<()> {
    if proto::read_u32(r).map_err(io_err)? != proto::MAGIC {
        return Err(proto::Error::new(Code::BadMagic, "bad magic"));
    }
    match Command::try_from(proto::read_u8(r).map_err(io_err)?) {
        Ok(Command::Get) => {
            let id = proto::read_uuid(r).map_err(io_err)?;
            let history = daemon.history();
            let entry = history.iter().find(|e| e.id == id).ok_or_else(not_found)?;
            entry.write(out).map_err(io_err)
        }
        Ok(Command::List) => {
            let amount = proto::read_u8(r).map_err(io_err)? as usize;
            let offset = proto::read_u32(r).map_err(io_err)? as usize;
            let history = daemon.history();
            out.push(history.len().saturating_sub(offset).min(amount) as u8);
            for entry in history.iter().skip(offset).take(amount) {
                entry.write(out).map_err(io_err)?;
            }
            Ok(())
        }
        Ok(Command::Set) => daemon.set(proto::read_uuid(r).map_err(io_err)?),
        Err(cmd) => Err(proto::Error::new(Code::UnknownCommand, format!("unknown command {cmd}"))),
    }
}

/// Reads the offer as text if possible, otherwise as its first mime type
fn receive(conn: &Connection, offer: &ZwlrDataControlOfferV1) -> Option<Entry> {
    let mimes = offer.data::<Mutex<Vec<String>>>()?.lock().unwrap();
    if mimes.iter().any(|m| m == MARKER_MIME) {
        return None;
    }
    let mime = mimes
        .iter()
        .find(|m| *m == "text/plain;charset=utf-8")
        .or_else(|| mimes.iter().find(|m| m.contains('/') && m.len() <= u8::MAX as usize))?
        .clone();

    let (mut read, write) = std::io::pipe().ok()?;
    offer.receive(mime.clone(), write.as_fd());
    // the fd is duplicated when queued, ours has to be closed to ever see EOF
    drop(write);
    conn.flush().ok()?;

    let mut data = Vec::new();
    read.read_to_end(&mut data).ok()?;
    let mut id = [0; 16];
    File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut id)).ok()?;
    (!data.is_empty()).then_some(Entry { id, mime, data })
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for Daemon {
    fn event(daemon: &mut Self, _: &ZwlrDataControlDeviceV1, event: device::Event, _: &(), conn: &Connection, _: &QueueHandle<Self>) {
        if let device::Event::Selection { id: Some(offer) } = event {
            if let Some(entry) = receive(conn, &offer) {
                let mut history = daemon.history();
                history.retain(|e| e.data != entry.data);
                history.push_front(entry);
                history.truncate(MAX_ENTRIES);
            }
            offer.destroy();
        }
    }

    event_created_child!(Daemon, ZwlrDataControlDeviceV1, [
        device::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, Mutex::new(Vec::new())),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, Mutex<Vec<String>>> for Daemon {
    fn event(_: &mut Self, _: &ZwlrDataControlOfferV1, event: offer::Event, mimes: &Mutex<Vec<String>>, _: &Connection, _: &QueueHandle<Self>) {
        if let offer::Event::Offer { mime_type } = event {
            mimes.lock().unwrap().push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, Entry> for Daemon {
    fn event(_: &mut Self, source: &ZwlrDataControlSourceV1, event: source::Event, entry: &Entry, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            source::Event::Send { mime_type, fd } if mime_type == entry.mime => {
                let _ = File::from(fd).write_all(&entry.data);
            }
            source::Event::Cancelled => source.destroy(),
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Daemon {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(Daemon: ZwlrDataControlManagerV1);
delegate_noop!(Daemon: ignore WlSeat);
