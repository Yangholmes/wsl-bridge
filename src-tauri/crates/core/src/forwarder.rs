use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwarderKind {
    Tcp,
    Udp,
    HttpProxy,
    Socks5Proxy,
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
    target_addr: Option<SocketAddr>,
) -> io::Result<ForwarderHandle> {
    match kind {
        ForwarderKind::Tcp => spawn_tcp_forwarder(
            listen_addr,
            target_addr
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing target"))?,
        ),
        ForwarderKind::Udp => spawn_udp_forwarder(
            listen_addr,
            target_addr
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing target"))?,
        ),
        ForwarderKind::HttpProxy => spawn_http_proxy_forwarder(listen_addr),
        ForwarderKind::Socks5Proxy => spawn_socks5_proxy_forwarder(listen_addr),
    }
}

fn spawn_tcp_forwarder(
    listen_addr: SocketAddr,
    target_addr: SocketAddr,
) -> io::Result<ForwarderHandle> {
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

fn spawn_udp_forwarder(
    listen_addr: SocketAddr,
    target_addr: SocketAddr,
) -> io::Result<ForwarderHandle> {
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
            if upstream
                .send_to(&recv_buf[..recv_len], target_addr)
                .is_err()
            {
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

fn spawn_http_proxy_forwarder(listen_addr: SocketAddr) -> io::Result<ForwarderHandle> {
    let listener = TcpListener::bind(listen_addr)?;
    listener.set_nonblocking(true)?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_loop = Arc::clone(&stop);

    let join = thread::spawn(move || {
        while !stop_loop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((inbound, _peer)) => {
                    thread::spawn(move || {
                        let _ = handle_http_proxy_connection(inbound);
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(80));
                }
                Err(_) => break,
            }
        }
    });

    Ok(ForwarderHandle {
        stop,
        join: Some(join),
    })
}

fn spawn_socks5_proxy_forwarder(listen_addr: SocketAddr) -> io::Result<ForwarderHandle> {
    let listener = TcpListener::bind(listen_addr)?;
    listener.set_nonblocking(true)?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_loop = Arc::clone(&stop);

    let join = thread::spawn(move || {
        while !stop_loop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((inbound, _peer)) => {
                    let stop_conn = Arc::clone(&stop_loop);
                    thread::spawn(move || {
                        let _ = handle_socks5_connection(inbound, stop_conn);
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(80));
                }
                Err(_) => break,
            }
        }
    });

    Ok(ForwarderHandle {
        stop,
        join: Some(join),
    })
}

fn handle_http_proxy_connection(inbound: TcpStream) -> io::Result<()> {
    inbound.set_nonblocking(false)?;
    let mut reader = BufReader::new(inbound);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let request_line_trimmed = request_line.trim_end_matches(['\r', '\n']);
    let mut parts = request_line_trimmed.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("HTTP/1.1");
    if method.is_empty() || target.is_empty() {
        return Ok(());
    }

    let mut headers: Vec<String> = Vec::new();
    let mut host_header: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.trim().eq_ignore_ascii_case("host") {
                host_header = Some(v.trim().to_owned());
            }
            if !k.trim().eq_ignore_ascii_case("proxy-connection") {
                headers.push(trimmed.to_owned());
            }
        } else {
            headers.push(trimmed.to_owned());
        }
    }

    let mut inbound = reader.into_inner();
    let mut outbound = if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = parse_host_port(target).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid CONNECT authority")
        })?;
        TcpStream::connect((host.as_str(), port))?
    } else {
        let (host, port, path) = resolve_http_request_target(target, host_header.as_deref())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid request target"))?;
        let mut upstream = TcpStream::connect((host.as_str(), port))?;
        write!(upstream, "{method} {path} {version}\r\n")?;
        for header in headers {
            write!(upstream, "{header}\r\n")?;
        }
        write!(upstream, "\r\n")?;
        upstream
    };
    inbound.set_nonblocking(false)?;
    outbound.set_nonblocking(false)?;

    if method.eq_ignore_ascii_case("CONNECT") {
        inbound.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    }

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

fn resolve_http_request_target(
    target: &str,
    host_header: Option<&str>,
) -> Option<(String, u16, String)> {
    if let Some(rest) = target.strip_prefix("http://") {
        let (authority, path) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        };
        let (host, port) = parse_host_port(authority)?;
        return Some((host, port, path.to_owned()));
    }

    let host = host_header?;
    let (host, port) = parse_host_port(host)?;
    let path = if target.is_empty() { "/" } else { target };
    Some((host, port, path.to_owned()))
}

fn parse_host_port(authority: &str) -> Option<(String, u16)> {
    let value = authority.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with('[') {
        let end = value.find(']')?;
        let host = &value[1..end];
        let port = if let Some(port_part) = value.get(end + 1..)?.strip_prefix(':') {
            port_part.parse::<u16>().ok()?
        } else {
            80
        };
        return Some((host.to_owned(), port));
    }
    if let Some((host, port)) = value.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            return Some((host.to_owned(), port));
        }
    }
    Some((value.to_owned(), 80))
}

fn handle_socks5_connection(mut inbound: TcpStream, stop: Arc<AtomicBool>) -> io::Result<()> {
    inbound.set_nonblocking(false)?;
    inbound.set_read_timeout(Some(Duration::from_secs(5)))?;

    let mut head = [0u8; 2];
    inbound.read_exact(&mut head)?;
    if head[0] != 0x05 {
        return Ok(());
    }
    let methods_len = head[1] as usize;
    let mut methods = vec![0u8; methods_len];
    inbound.read_exact(&mut methods)?;
    let supports_no_auth = methods.contains(&0x00);
    if !supports_no_auth {
        inbound.write_all(&[0x05, 0xFF])?;
        return Ok(());
    }
    inbound.write_all(&[0x05, 0x00])?;

    let mut req_head = [0u8; 4];
    inbound.read_exact(&mut req_head)?;
    if req_head[0] != 0x05 {
        return Ok(());
    }
    let cmd = req_head[1];
    let atyp = req_head[3];
    let target_host = read_socks_addr(&mut inbound, atyp)?;
    let mut port_buf = [0u8; 2];
    inbound.read_exact(&mut port_buf)?;
    let target_port = u16::from_be_bytes(port_buf);

    match cmd {
        0x01 => {
            let mut outbound = TcpStream::connect((target_host.as_str(), target_port))?;
            write_socks_success(&mut inbound, outbound.local_addr().ok())?;

            inbound.set_read_timeout(None)?;
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
        0x03 => {
            let udp_socket = UdpSocket::bind(("0.0.0.0", 0))?;
            udp_socket.set_read_timeout(Some(Duration::from_millis(200)))?;
            write_socks_success(&mut inbound, udp_socket.local_addr().ok())?;

            let relay_stop = Arc::new(AtomicBool::new(false));
            let relay_stop_loop = Arc::clone(&relay_stop);
            let client_ip = inbound
                .peer_addr()
                .map(|addr| addr.ip())
                .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

            let relay_join = thread::spawn(move || {
                run_socks5_udp_relay(udp_socket, client_ip, relay_stop_loop);
            });

            inbound.set_read_timeout(Some(Duration::from_millis(250)))?;
            let mut probe = [0u8; 1];
            while !stop.load(Ordering::Relaxed) {
                match inbound.read(&mut probe) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(err)
                        if err.kind() == io::ErrorKind::WouldBlock
                            || err.kind() == io::ErrorKind::TimedOut =>
                    {
                        continue;
                    }
                    Err(_) => break,
                }
            }
            relay_stop.store(true, Ordering::Relaxed);
            let _ = relay_join.join();
            Ok(())
        }
        _ => {
            write_socks_reply(&mut inbound, 0x07, None)?;
            Ok(())
        }
    }
}

fn run_socks5_udp_relay(socket: UdpSocket, client_ip: IpAddr, stop: Arc<AtomicBool>) {
    let mut client_udp_addr: Option<SocketAddr> = None;
    let mut recv_buf = [0u8; 65535];
    while !stop.load(Ordering::Relaxed) {
        let (len, src) = match socket.recv_from(&mut recv_buf) {
            Ok(value) => value,
            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        };

        let is_client_packet = if let Some(client_addr) = client_udp_addr {
            src == client_addr
        } else {
            src.ip() == client_ip
        };

        if is_client_packet {
            if let Some((target_addr, payload)) = parse_socks5_udp_packet(&recv_buf[..len]) {
                client_udp_addr = Some(src);
                let _ = socket.send_to(payload, target_addr);
            }
            continue;
        }

        if let Some(client_addr) = client_udp_addr {
            let packet = build_socks5_udp_packet(src, &recv_buf[..len]);
            let _ = socket.send_to(&packet, client_addr);
        }
    }
}

fn read_socks_addr(stream: &mut TcpStream, atyp: u8) -> io::Result<String> {
    match atyp {
        0x01 => {
            let mut v4 = [0u8; 4];
            stream.read_exact(&mut v4)?;
            Ok(Ipv4Addr::from(v4).to_string())
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len)?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain)?;
            Ok(String::from_utf8_lossy(&domain).to_string())
        }
        0x04 => {
            let mut v6 = [0u8; 16];
            stream.read_exact(&mut v6)?;
            Ok(std::net::Ipv6Addr::from(v6).to_string())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported atyp",
        )),
    }
}

fn write_socks_success(stream: &mut TcpStream, bind_addr: Option<SocketAddr>) -> io::Result<()> {
    write_socks_reply(stream, 0x00, bind_addr)
}

fn write_socks_reply(
    stream: &mut TcpStream,
    rep: u8,
    bind_addr: Option<SocketAddr>,
) -> io::Result<()> {
    let addr = bind_addr.unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));
    match addr.ip() {
        IpAddr::V4(v4) => {
            let mut reply = vec![0x05, rep, 0x00, 0x01];
            reply.extend_from_slice(&v4.octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
            stream.write_all(&reply)?;
        }
        IpAddr::V6(v6) => {
            let mut reply = vec![0x05, rep, 0x00, 0x04];
            reply.extend_from_slice(&v6.octets());
            reply.extend_from_slice(&addr.port().to_be_bytes());
            stream.write_all(&reply)?;
        }
    }
    Ok(())
}

fn parse_socks5_udp_packet(packet: &[u8]) -> Option<(SocketAddr, &[u8])> {
    if packet.len() < 4 {
        return None;
    }
    if packet[0] != 0x00 || packet[1] != 0x00 {
        return None;
    }
    if packet[2] != 0x00 {
        return None;
    }

    let mut idx = 3usize;
    let atyp = *packet.get(idx)?;
    idx += 1;
    let host = match atyp {
        0x01 => {
            let bytes = packet.get(idx..idx + 4)?;
            idx += 4;
            Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string()
        }
        0x03 => {
            let len = *packet.get(idx)? as usize;
            idx += 1;
            let bytes = packet.get(idx..idx + len)?;
            idx += len;
            String::from_utf8_lossy(bytes).to_string()
        }
        0x04 => {
            let bytes = packet.get(idx..idx + 16)?;
            idx += 16;
            let mut octets = [0u8; 16];
            octets.copy_from_slice(bytes);
            std::net::Ipv6Addr::from(octets).to_string()
        }
        _ => return None,
    };
    let port_bytes = packet.get(idx..idx + 2)?;
    idx += 2;
    let port = u16::from_be_bytes([port_bytes[0], port_bytes[1]]);
    let payload = packet.get(idx..)?;
    let target = (host.as_str(), port).to_socket_addrs().ok()?.next()?;
    Some((target, payload))
}

fn build_socks5_udp_packet(src: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 32);
    out.extend_from_slice(&[0x00, 0x00, 0x00]);
    match src.ip() {
        IpAddr::V4(v4) => {
            out.push(0x01);
            out.extend_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            out.push(0x04);
            out.extend_from_slice(&v6.octets());
        }
    }
    out.extend_from_slice(&src.port().to_be_bytes());
    out.extend_from_slice(payload);
    out
}
