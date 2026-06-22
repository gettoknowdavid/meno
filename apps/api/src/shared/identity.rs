/// Narrow, read-only view into user identity for modules that just need to
/// validate a user exists — they should not depend on the full `AuthRepo`
/// surface (refresh tokens, password hashes, OTPs, etc).
#[async_trait::async_trait]
pub trait IdentityReader: Send + Sync + 'static {
    async fn find_user_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<crate::modules::auth::model::User>, sqlx::Error>;
}
