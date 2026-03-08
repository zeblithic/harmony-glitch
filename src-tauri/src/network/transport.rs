use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Default port for Reticulum LAN discovery.
pub const DEFAULT_PORT: u16 = 29170;

/// Non-blocking UDP transport for LAN discovery and data.
pub struct UdpTransport {
    socket: UdpSocket,
    recv_buf: Vec<u8>,
}

impl UdpTransport {
    /// Bind to the given port. Sets socket to non-blocking.
    /// Enables SO_REUSEADDR for multiple instances on same machine (dev/testing).
    pub fn bind(port: u16) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        socket.bind(&SocketAddr::from(([0, 0, 0, 0], port)).into())?;
        Ok(Self {
            socket: socket.into(),
            recv_buf: vec![0u8; 600], // > Reticulum MTU (500)
        })
    }

    /// Returns the local address this transport is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Read all available packets (non-blocking). Returns (data, source_addr) pairs.
    pub fn recv_all(&mut self) -> Vec<(Vec<u8>, SocketAddr)> {
        let mut packets = Vec::new();
        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((len, addr)) => {
                    packets.push((self.recv_buf[..len].to_vec(), addr));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("[UdpTransport] recv_from error: {e}");
                    break;
                }
            }
        }
        packets
    }

    /// Send data to a specific address.
    pub fn send_to(&self, data: &[u8], addr: SocketAddr) -> io::Result<usize> {
        self.socket.send_to(data, addr)
    }

    /// Broadcast data on the LAN (255.255.255.255:port).
    pub fn broadcast(&self, data: &[u8], port: u16) -> io::Result<usize> {
        self.socket
            .send_to(data, SocketAddr::from(([255, 255, 255, 255], port)))
    }
}

/// Optional TCP connection for WAN relay.
pub struct TcpRelay {
    // TODO: implement in a follow-up step.
    // For Phase B MVP, LAN UDP is sufficient.
    // TCP relay connects to a known harmony-node transport.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_and_reads_without_blocking() {
        let mut transport = UdpTransport::bind(0).unwrap(); // port 0 = OS picks
        let packets = transport.recv_all();
        assert!(packets.is_empty()); // No data sent, no data received
    }

    #[test]
    fn send_and_receive_loopback() {
        let mut t1 = UdpTransport::bind(0).unwrap();
        let t2 = UdpTransport::bind(0).unwrap();

        // local_addr() returns 0.0.0.0:port when bound to INADDR_ANY;
        // we need to target 127.0.0.1 for loopback delivery.
        let port = t1.local_addr().unwrap().port();
        let addr1 = SocketAddr::from(([127, 0, 0, 1], port));
        t2.send_to(b"hello", addr1).unwrap();

        // Brief pause for OS to deliver — non-blocking read
        std::thread::sleep(std::time::Duration::from_millis(10));

        let packets = t1.recv_all();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, b"hello");
    }
}
