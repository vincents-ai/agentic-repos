use crate::git::error::{GitError, Result};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, error};

pub struct GitProtocolHandler;

impl GitProtocolHandler {
    pub fn new() -> Self {
        Self
    }

    /// Handle git-upload-pack command
    pub async fn handle_upload_pack<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        _repo_path: &str,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        debug!("Starting git-upload-pack protocol handler");

        // Read the initial pkt-line request
        let mut initial_request = vec![0u8; 1024];
        let n = reader.read(&mut initial_request).await.map_err(|e| {
            error!("Failed to read upload-pack request: {}", e);
            GitError::ProtocolError(format!("Failed to read request: {}", e))
        })?;

        if n == 0 {
            return Err(GitError::ProtocolError("Empty request".to_string()));
        }

        let request_str = String::from_utf8_lossy(&initial_request[..n]);
        debug!("Received upload-pack request: {}", request_str);

        // TODO: Implement full Git Smart Protocol using gix-protocol
        // For now, send a basic response indicating the service is available

        // Send service advertisement
        let service_advertisement = format!("{}#{}\n", "# service=git-upload-pack", "0000");

        writer
            .write_all(service_advertisement.as_bytes())
            .await
            .map_err(|e| {
                GitError::ProtocolError(format!("Failed to write advertisement: {}", e))
            })?;

        debug!("Upload-pack handler initialized");
        Ok(())
    }

    /// Handle git-receive-pack command
    pub async fn handle_receive_pack<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        _repo_path: &str,
    ) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        debug!("Starting git-receive-pack protocol handler");

        // Read the initial pkt-line request
        let mut initial_request = vec![0u8; 1024];
        let n = reader.read(&mut initial_request).await.map_err(|e| {
            error!("Failed to read receive-pack request: {}", e);
            GitError::ProtocolError(format!("Failed to read request: {}", e))
        })?;

        if n == 0 {
            return Err(GitError::ProtocolError("Empty request".to_string()));
        }

        let request_str = String::from_utf8_lossy(&initial_request[..n]);
        debug!("Received receive-pack request: {}", request_str);

        // TODO: Implement full Git Smart Protocol using gix-protocol
        // For now, send a basic response indicating the service is available

        // Send service advertisement
        let service_advertisement = format!("{}#{}\n", "# service=git-receive-pack", "0000");

        writer
            .write_all(service_advertisement.as_bytes())
            .await
            .map_err(|e| {
                GitError::ProtocolError(format!("Failed to write advertisement: {}", e))
            })?;

        debug!("Receive-pack handler initialized");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_protocol_handler_creation() {
        let handler = GitProtocolHandler::new();
        assert!(handler
            .handle_upload_pack(&mut &[0u8; 1][..], &mut vec![], "/test/repo.git")
            .await
            .is_ok());
    }
}
