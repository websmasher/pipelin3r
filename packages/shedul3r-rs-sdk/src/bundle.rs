//! Bundle upload and download utilities.
//!
//! These endpoints support remote execution workflows where task inputs and
//! outputs are transferred as file bundles. The API is defined but not yet
//! implemented on the server side.

/// Opaque handle returned after uploading a bundle.
#[derive(Debug, Clone)]
pub struct BundleHandle {
    /// Server-assigned identifier for the uploaded bundle.
    pub id: String,
    /// Remote path where the bundle is stored.
    pub remote_path: String,
}

impl super::Client {
    /// Upload a bundle of files. Returns a handle with the remote path.
    ///
    /// # Errors
    ///
    /// Currently always returns an error — bundle endpoints are not yet
    /// implemented on the server.
    #[allow(clippy::unused_async)] // stub — will use await when server endpoint exists
    pub async fn upload_bundle(
        &self,
        _files: &[(&str, &[u8])],
    ) -> anyhow::Result<BundleHandle> {
        anyhow::bail!("bundle upload endpoint not yet implemented")
    }

    /// Download a file from a previously uploaded bundle.
    ///
    /// # Errors
    ///
    /// Currently always returns an error — bundle endpoints are not yet
    /// implemented on the server.
    #[allow(clippy::unused_async)] // stub — will use await when server endpoint exists
    pub async fn download_file(
        &self,
        _bundle_id: &str,
        _path: &str,
    ) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("bundle download endpoint not yet implemented")
    }

    /// Delete a previously uploaded bundle.
    ///
    /// # Errors
    ///
    /// Currently always returns an error — bundle endpoints are not yet
    /// implemented on the server.
    #[allow(clippy::unused_async)] // stub — will use await when server endpoint exists
    pub async fn delete_bundle(&self, _bundle_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("bundle delete endpoint not yet implemented")
    }
}
