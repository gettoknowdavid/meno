use crate::config::MenoConfig;
use anyhow::Result;
use axum::http::Method;
use object_store::ObjectStoreExt;
use object_store::path::Path;
use object_store::signer::Signer;
use std::sync::Arc;

#[derive(Clone)]
pub struct StorageService {
    store: Arc<object_store::aws::AmazonS3>,
    pub bucket: String,
    pub public_url: String,
}
impl StorageService {
    pub fn new(config: &MenoConfig) -> Self {
        let store = object_store::aws::AmazonS3Builder::new()
            .with_endpoint(&config.storage_endpoint)
            .with_access_key_id(&config.storage_access_key)
            .with_secret_access_key(&config.storage_secret_key)
            .with_bucket_name(&config.storage_bucket)
            .with_region(&config.storage_region)
            .with_virtual_hosted_style_request(false)
            .with_allow_http(true)
            .build()
            .expect("Failed to build storage client");

        Self {
            store: Arc::new(store),
            bucket: config.storage_bucket.clone(),
            public_url: config.storage_public_url.clone(),
        }
    }

    /// Generates a presigned PUT URL the client uses to upload directly.
    /// The client does: PUT <url> with the file bytes as body.
    /// Expires in 10 minutes — enough time for the user to hit "Save changes".
    pub async fn presigned_upload_url(&self, object_key: &str) -> Result<String> {
        let path = Path::from(object_key);
        let expiry = std::time::Duration::from_secs(600);

        let url = self.store.signed_url(Method::PUT, &path, expiry).await?;
        Ok(url.to_string())
    }

    /// Verifies a key actually exists in storage before accepting it.
    /// Called during PATCH /users/me to validate the avatar_key the client sent.
    pub async fn object_exists(&self, object_key: &str) -> Result<bool> {
        let path = Path::from(object_key);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Deletes an old avatar when the user uploads a new one.
    /// Idempotent — NotFound is not an error.
    pub async fn delete(&self, object_key: &str) -> Result<()> {
        let path = Path::from(object_key);
        match self.store.delete(&path).await {
            Ok(_) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Constructs the public URL for a stored object.
    /// For avatars: {public_url}/avatars/{user_id}/{filename}
    pub fn public_url_for(&self, object_key: &str) -> String {
        format!("{}/{}", self.public_url, object_key)
    }
}
