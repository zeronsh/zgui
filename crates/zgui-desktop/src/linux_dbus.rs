//! A session bus connection served on a thread of its own: the thread sleeps
//! until a message arrives or it is told to stop, where a loop of
//! `Connection::process` with a timeout wakes several times a second to look
//! at a flag, with nothing to do.
use dbus::{
    blocking::Connection,
    channel::{BusType, Channel},
};
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::Duration,
};

/// A connection to the session bus whose descriptor `serve` can wait on.
pub(crate) fn session() -> Result<Connection, dbus::Error> {
    let mut channel = Channel::get_private(BusType::Session)?;
    channel.set_watch_enabled(true);
    Ok(channel.into())
}

/// Tells a thread in `serve` to return, from any thread; once told, it
/// stays told.
pub(crate) struct Stop(OwnedFd);

impl Stop {
    pub fn new() -> Option<Self> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        // SAFETY: a descriptor of ours, closed with this.
        (fd >= 0).then(|| Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub fn stop(&self) {
        let one = 1u64;
        unsafe { libc::write(self.0.as_raw_fd(), (&one as *const u64).cast(), 8) };
    }
}

/// Handle `connection`'s messages (its matches and receivers) until `stop`
/// is told or the bus goes; between them the thread sleeps. The connection
/// is one of `session`'s.
pub(crate) fn serve(connection: &Connection, stop: &Stop) {
    while wait(connection, stop) {}
}

/// Handle what has arrived on `connection`; if nothing had, sleep until
/// something does or `stop` is told. False once told or the bus goes. A
/// caller waiting for a reply its handlers record looks for it after each.
pub(crate) fn wait(connection: &Connection, stop: &Stop) -> bool {
    // What has arrived, read without waiting; then what handling it sent
    // goes out.
    let mut handled = false;
    loop {
        match connection.process(Duration::ZERO) {
            Ok(true) => handled = true,
            Ok(false) => break,
            Err(_) => return false,
        }
    }
    connection.channel().flush();
    if handled {
        return true;
    }
    let watch = connection.channel().watch();
    let mut fds = [
        libc::pollfd {
            fd: watch.fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: stop.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
    !(fds[1].revents != 0
        || (n < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted))
}

#[cfg(test)]
mod tests {
    use super::{Stop, serve, session};
    use dbus::{Message, channel::Sender, message::MatchRule};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };

    /// A thread's voluntary context switches: how often it slept and woke.
    fn switches(tid: libc::pid_t) -> u64 {
        std::fs::read_to_string(format!("/proc/self/task/{tid}/status"))
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("voluntary_ctxt_switches:"))
            .and_then(|n| n.trim().parse().ok())
            .unwrap()
    }

    #[test]
    fn a_served_connection_sleeps_until_a_signal_or_stop() {
        let (Ok(served), Ok(sender)) = (session(), session()) else {
            return; // No session bus here.
        };
        let (heard, hear) = mpsc::channel();
        let rule = MatchRule::new_signal("org.zgui.Test", "Ping");
        served
            .add_match(rule, move |(): (), _, _| heard.send(()).is_ok())
            .unwrap();
        let stop = Arc::new(Stop::new().unwrap());
        let told = stop.clone();
        let (tid_of, tid) = mpsc::channel();
        let serving = std::thread::spawn(move || {
            tid_of.send(unsafe { libc::gettid() }).unwrap();
            serve(&served, &told);
        });
        let tid = tid.recv().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        // Asleep: no timeout wakes it meanwhile.
        let before = switches(tid);
        std::thread::sleep(Duration::from_millis(600));
        assert!(
            switches(tid) - before <= 1,
            "the served thread woke while idle"
        );

        let ping = Message::new_signal("/org/zgui/Test", "org.zgui.Test", "Ping").unwrap();
        sender.send(ping).unwrap();
        sender.channel().flush();
        hear.recv_timeout(Duration::from_secs(5)).unwrap();

        stop.stop();
        let started = std::time::Instant::now();
        serving.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
