use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwarderKind {
    Tcp,
    Udp,
}

#[derive(Debug)]
pub struct ForwarderHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ForwarderHandle {
    pub fn stop_and_join(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn spawn(
    kind: ForwarderKind,
    listen_addr: SocketAddr,
    target_addr: SocketAddr,
) -> io::Result<ForwarderHandle> {
    match kind {
        ForwarderKind::Tcp => spawn_tcp_forwarder(listen_addr, target_addr),
        ForwarderKind::Udp => spawn_udp_forwarder(listen_addr, target_addr),
    }
}

fn spawn_tcp_forwarder(listen_addr: SocketAddr, target_addr: SocketAddr) -> io::Result<ForwarderHandle> {
    let listener = TcpListener::bind(listen_addr)?;
    listener.set_nonblocking(true)?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_loop = Arc::clone(&stop);

    let join = thread::spawn(move || {
        while !stop_loop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((inbound, _peer)) => {
                    thread::spawn(move || {
                        let _ = handle_tcp_connection(inbound, target_addr);
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(80));
                }
                Err(_) => {
                    break;
                }
            }
        }
    });

    Ok(ForwarderHandle {
        stop,
        join: Some(join),
    })
}

fn handle_tcp_connection(mut inbound: TcpStream, target_addr: SocketAddr) -> io::Result<()> {
    let mut outbound = TcpStream::connect(target_addr)?;
    inbound.set_nonblocking(false)?;
    outbound.set_nonblocking(false)?;

    let mut inbound_clone = inbound.try_clone()?;
    let mut outbound_clone = outbound.try_clone()?;

    let left = thread::spawn(move || {
        let _ = io::copy(&mut inbound_clone, &mut outbound);
    });

    let right = thread::spawn(move || {
        let _ = io::copy(&mut outbound_clone, &mut inbound);
    });

    let _ = left.join();
    let _ = right.join();
    Ok(())
}

fn spawn_udp_forwarder(listen_addr: SocketAddr, target_addr: SocketAddr) -> io::Result<ForwarderHandle> {
    let socket = UdpSocket::bind(listen_addr)?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_loop = Arc::clone(&stop);

    let join = thread::spawn(move || {
        let mut recv_buf = [0u8; 65535];
        while !stop_loop.load(Ordering::Relaxed) {
            let (recv_len, client_addr) = match socket.recv_from(&mut recv_buf) {
                Ok(v) => v,
                Err(err)
                    if err.kind() == io::ErrorKind::WouldBlock
                        || err.kind() == io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(_) => break,
            };

            let upstream = match UdpSocket::bind(("0.0.0.0", 0)) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let _ = upstream.set_read_timeout(Some(Duration::from_millis(350)));
            if upstream.send_to(&recv_buf[..recv_len], target_addr).is_err() {
                continue;
            }

            let mut response_buf = [0u8; 65535];
            if let Ok((resp_len, _)) = upstream.recv_from(&mut response_buf) {
                let _ = socket.send_to(&response_buf[..resp_len], client_addr);
            }
        }
    });

    Ok(ForwarderHandle {
        stop,
        join: Some(join),
    })
}
