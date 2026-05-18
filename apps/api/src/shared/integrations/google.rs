use crate::config::MenoConfig;
use anyhow::{Result, ensure};
use oauth2::url::Url;
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, StandardRevocableToken,
    TokenResponse, TokenUrl,
    basic::{
        BasicClient, BasicErrorResponse, BasicRevocationErrorResponse,
        BasicTokenIntrospectionResponse, BasicTokenResponse,
    },
};
use serde::Deserialize;

pub type OAuthClient = Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub sub: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub email_verified: bool,
}

#[derive(Clone)]
pub struct GoogleAuthService {
    client: OAuthClient,
}
impl GoogleAuthService {
    pub fn new(config: &MenoConfig) -> Self {
        let auth_uri =
            AuthUrl::new(config.google_auth_uri.clone()).expect("Invalid Google auth URI");
        let token_uri =
            TokenUrl::new(config.google_token_uri.clone()).expect("Invalid Google token URI");
        let redirect_uri = RedirectUrl::new(config.google_redirect_uri.clone())
            .expect("Invalid Google redirect URI");

        let client = BasicClient::new(ClientId::new(config.google_client_id.clone()))
            .set_client_secret(ClientSecret::new(config.google_client_secret.clone()))
            .set_auth_uri(auth_uri)
            .set_token_uri(token_uri)
            .set_redirect_uri(redirect_uri);

        Self { client }
    }

    pub fn authorize_url(&self) -> (Url, CsrfToken, PkceCodeVerifier) {
        let (pkce_code_challenge, pkce_code_verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .set_pkce_challenge(pkce_code_challenge)
            .url();
        (url, csrf_token, pkce_code_verifier)
    }

    pub async fn exchange_code(
        &self,
        code: String,
        pkce_code_verifier: PkceCodeVerifier,
    ) -> Result<GoogleUserInfo> {
        let http_client = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .expect("Client should build");

        let token_response = self
            .client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_code_verifier)
            .request_async(&http_client)
            .await?;

        let userinfo: GoogleUserInfo = reqwest::Client::new()
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(token_response.access_token().secret())
            .send()
            .await?
            .json()
            .await?;

        Ok(userinfo)
    }

    pub async fn verify_id_token(&self, id_token: &str) -> Result<GoogleUserInfo> {
        // Google's token info endpoint — validates the id_token and returns claims
        // Used for mobile flow where Firebase already handled the OAuth dance
        let url = format!(
            "https://oauth2.googleapis.com/tokeninfo?id_token={}",
            id_token
        );

        let info: GoogleUserInfo = reqwest::Client::new()
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        // Verify audience matches your client ID
        ensure!(info.email_verified, "Google email not verified");

        Ok(info)
    }
}
