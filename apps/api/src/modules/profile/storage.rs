use crate::modules::profile::errors::ProfileError;
use crate::shared::services::storage::StorageService;

#[derive(Clone)]
pub struct ProfileStorage {
    storage: StorageService,
}
impl ProfileStorage {
    pub fn new(storage: StorageService) -> Self {
        Self { storage }
    }

    pub async fn object_exists(&self, key: &str) -> Result<bool, ProfileError> {
        self.storage
            .object_exists(key)
            .await
            .map_err(|e| ProfileError::Storage(e.to_string()))
    }
    pub fn get_avatar_url(&self, key: &str) -> String {
        self.storage.public_url_for(key)
    }
    pub async fn get_avatar_upload_url(&self, avatar_id: &str) -> Result<String, ProfileError> {
        self.storage
            .presigned_upload_url(avatar_id)
            .await
            .map_err(|e| ProfileError::Storage(e.to_string()))
    }
    pub async fn delete_avatar(&self, key: &str) -> Result<(), ProfileError> {
        self.storage
            .delete(key)
            .await
            .map_err(|e| ProfileError::Storage(e.to_string()))
    }
}
