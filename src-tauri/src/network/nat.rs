use serde::Deserialize;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct StunReflexiveResponse {
    reflexive_addr: String,
}

/// Binds a reusable UDP socket (`SO_REUSEADDR`) on `0.0.0.0:port` so STUN discovery,
/// UDP hole-punching, and QUIC/UDP datagrams share the same NAT mapping.
pub fn bind_reusable_udp_socket(port: u16) -> Result< std::net::UdpSocket, String> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| e.to_string())?;
    let _ = socket.set_reuse_address(true);
    let addr: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    socket.bind(&addr.into()).map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    Ok(socket.into())
}

/// Queries the Raydrop STUN reflector on `stun_addr` (e.g., `signal.yourdomain.com:3478`)
/// using the local port to discover the public NAT reflexive `IP:Port`.
pub async fn discover_wan_reflexive_address(
    local_port: u16,
    stun_addr: &str,
) -> Option<String> {
    let std_sock = bind_reusable_udp_socket(local_port).ok()?;
    let sock = UdpSocket::from_std(std_sock).ok()?;

    let Ok(mut resolved) = tokio::net::lookup_host(stun_addr).await else {
        return None;
    };
    let target = resolved.find(|a| a.is_ipv4()).or_else(|| resolved.next())?;

    let payload = b"RAYD_STUN";
    let _ = sock.send_to(payload, target).await.ok()?;

    let mut buf = [0u8; 512];
    match tokio::time::timeout(Duration::from_millis(1800), sock.recv_from(&mut buf)).await {
        Ok(Ok((len, _))) => {
            let text = String::from_utf8_lossy(&buf[..len]);
            if let Ok(parsed) = serde_json::from_str::<StunReflexiveResponse>(&text) {
                info!("Discovered WAN reflexive address: {}", parsed.reflexive_addr);
                return Some(parsed.reflexive_addr);
            }
            let trimmed = text.trim().to_string();
            if trimmed.contains(':') {
                return Some(trimmed);
            }
            None
        }
        _ => {
            warn!("STUN discovery timed out for {stun_addr}");
            None
        }
    }
}

/// Sends a burst of UDP hole-punching packets (`RAYD_PUNCH:<session_id>`) from `local_port`
/// to `peer_wan_addr` to open a bidirectional pinhole on stateful NAT routers.
pub async fn execute_udp_hole_punch(
    local_port: u16,
    peer_wan_addr: &str,
    session_id: &str,
) {
    let Ok(target): Result<SocketAddr, _> = peer_wan_addr.parse() else {
        return;
    };
    let Ok(std_sock) = bind_reusable_udp_socket(local_port) else {
        return;
    };
    let Ok(sock) = UdpSocket::from_std(std_sock) else {
        return;
    };

    let packet = format!("RAYD_PUNCH:{session_id}");
    info!("Executing simultaneous UDP hole-punch to {target} for session {session_id}");
    for _ in 0..6 {
        let _ = sock.send_to(packet.as_bytes(), target).await;
        tokio::time::sleep(Duration::from_millis(90)).await;
    }
}

