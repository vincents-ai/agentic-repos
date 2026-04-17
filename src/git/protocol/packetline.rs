//! Git packet-line protocol utilities
//!
//! Implements the pkt-line format used by Git Smart Protocol:
//! - 4-byte hex length prefix (includes the 4 bytes itself)
//! - followed by data
//! - special packets: "0000" (flush), "0001" (delim), "0002" (response-end)

use crate::git::error::{GitError, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Special packet types
pub const PKT_FLUSH: &[u8] = b"0000";
#[allow(dead_code)]
pub const PKT_DELIM: &[u8] = b"0001";
#[allow(dead_code)]
pub const PKT_RESPONSE_END: &[u8] = b"0002";

/// Maximum packet size (64KB - 4 bytes for length prefix)
const MAX_PACKET_SIZE: usize = 65516;

/// Write a data packet with length prefix
pub async fn write_data_packet<W: AsyncWrite + Unpin>(writer: &mut W, data: &[u8]) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if data.len() > MAX_PACKET_SIZE {
        return Err(GitError::ProtocolError(format!(
            "Packet too large: {} bytes (max: {})",
            data.len(),
            MAX_PACKET_SIZE
        )));
    }

    let len = data.len() + 4; // Include the 4-byte length prefix
    let len_hex = format!("{:04x}", len);

    // Combine prefix and data into a single buffer to avoid multiple SSH packets
    let mut packet = Vec::with_capacity(len);
    packet.extend_from_slice(len_hex.as_bytes());
    packet.extend_from_slice(data);

    writer.write_all(&packet).await?;

    Ok(())
}

/// Write a flush packet (0000)
pub async fn write_flush_packet<W: AsyncWrite + Unpin>(writer: &mut W) -> Result<()> {
    writer.write_all(PKT_FLUSH).await?;
    Ok(())
}

/// Write a text packet (data + newline)
pub async fn write_text_packet<W: AsyncWrite + Unpin>(writer: &mut W, text: &str) -> Result<()> {
    let mut data = text.as_bytes().to_vec();
    data.push(b'\n');
    write_data_packet(writer, &data).await
}

/// Read a single packet
pub async fn read_packet<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    tracing::debug!("[PKTLINE] Reading packet header...");
    let mut len_buf = [0u8; 4];

    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            tracing::debug!("[PKTLINE] Got UnexpectedEof reading header");
            return Ok(None);
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            tracing::debug!("[PKTLINE] Got WouldBlock reading header");
            return Err(GitError::IoError(e));
        }
        Err(e) => {
            tracing::debug!("[PKTLINE] Got error reading header: {:?}", e);
            return Err(GitError::IoError(e));
        }
    }

    let len_str = std::str::from_utf8(&len_buf)
        .map_err(|e| GitError::ProtocolError(format!("Invalid packet length prefix: {}", e)))?;

    tracing::debug!("[PKTLINE] Got length prefix: {:?}", len_str);

    let len = u16::from_str_radix(len_str, 16)
        .map_err(|e| GitError::ProtocolError(format!("Invalid packet length hex: {}", e)))?
        as usize;

    tracing::debug!("[PKTLINE] Parsed length: {}", len);

    // Special packets
    if len == 0 {
        tracing::debug!("[PKTLINE] Flush packet");
        return Ok(Some(Vec::new())); // Flush packet
    }
    if len == 1 || len == 2 {
        tracing::debug!("[PKTLINE] Special packet: {}", len);
        return Ok(Some(vec![len as u8])); // Delim or response-end
    }

    if len < 4 {
        return Err(GitError::ProtocolError(format!(
            "Invalid packet length: {}",
            len
        )));
    }

    let data_len = len - 4;
    tracing::debug!("[PKTLINE] Reading {} bytes of data...", data_len);

    if data_len > MAX_PACKET_SIZE {
        return Err(GitError::ProtocolError(format!(
            "Packet too large: {} bytes",
            data_len
        )));
    }

    let mut data = vec![0u8; data_len];
    reader.read_exact(&mut data).await?;

    Ok(Some(data))
}

/// Read packets until flush packet
#[allow(dead_code)]
pub async fn read_until_flush<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<Vec<u8>>> {
    let mut packets = Vec::new();

    loop {
        match read_packet(reader).await? {
            None => break,
            Some(data) if data.is_empty() => break, // Flush packet
            Some(data) => packets.push(data),
        }
    }

    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_read_packet() {
        let mut buffer = Vec::new();
        let data = b"hello world";

        write_data_packet(&mut buffer, data).await.unwrap();

        let mut reader = &buffer[..];
        let result = read_packet(&mut reader).await.unwrap();

        assert_eq!(result, Some(data.to_vec()));
    }

    #[tokio::test]
    async fn test_flush_packet() {
        let mut buffer = Vec::new();
        write_flush_packet(&mut buffer).await.unwrap();

        assert_eq!(&buffer, PKT_FLUSH);

        let mut reader = &buffer[..];
        let result = read_packet(&mut reader).await.unwrap();
        assert_eq!(result, Some(Vec::new()));
    }

    #[tokio::test]
    async fn test_text_packet() {
        let mut buffer = Vec::new();
        write_text_packet(&mut buffer, "test message")
            .await
            .unwrap();

        let mut reader = &buffer[..];
        let result = read_packet(&mut reader).await.unwrap().unwrap();

        assert_eq!(result, b"test message\n");
    }

    #[tokio::test]
    async fn test_read_until_flush() {
        let mut buffer = Vec::new();
        write_data_packet(&mut buffer, b"packet1").await.unwrap();
        write_data_packet(&mut buffer, b"packet2").await.unwrap();
        write_flush_packet(&mut buffer).await.unwrap();

        let mut reader = &buffer[..];
        let packets = read_until_flush(&mut reader).await.unwrap();

        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], b"packet1");
        assert_eq!(packets[1], b"packet2");
    }
}
