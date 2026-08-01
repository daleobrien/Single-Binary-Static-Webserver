use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};
use tokio::net::TcpListener;

pub(crate) fn create_reuseport_listener(
    port: u16,
) -> Result<TcpListener, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let std_listener: std::net::TcpListener = socket.into();
    Ok(TcpListener::from_std(std_listener)?)
}

pub(crate) fn create_reuseport_udp_socket(
    port: u16,
) -> Result<UdpSocket, Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = format!("[::]:{port}").parse()?;
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_only_v6(false)?;
    socket.set_reuse_port(true)?;
    socket.bind(&addr.into())?;
    let std_socket: UdpSocket = socket.into();
    Ok(std_socket)
}
