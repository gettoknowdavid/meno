//! A throwaway RSA service-account key, for tests that must really sign a JWT.
//!
//! # Why a key is checked in
//!
//! [`AccessTokenProvider`] signs an RS256 assertion on every refresh, so testing it
//! needs a *real* RSA private key. Generating one at test time would mean an RSA
//! key-generation dependency, which is a heavier thing to pull into a test suite than a
//! 1.7 KB fixture. A fixed key also makes the tests deterministic and fully offline.
//!
//! The key was generated for this repository with `openssl genpkey` and is used by
//! nothing but these tests. It is not the credential of any Firebase or Google project,
//! and cannot become one: an assertion it signs is only accepted by a token endpoint
//! that trusts the matching public key.
//!
//! # Why the JSON is assembled rather than pasted
//!
//! The token exchange is only testable offline if `token_uri` can point at a local
//! server, and hand-editing a JSON fixture per test is how a test ends up asserting
//! against the wrong endpoint.

/// A 2048-bit RSA private key in PKCS#8 PEM form. Test-only; see the module docs.
pub(crate) const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDRzpH5pEymMluF
aaqJEMBXQdtgUZbj/q4eojrEzpIKcNKW9RiJRjxSWMRtN8b2QqiVQuZK616blnTl
aIkbr1twBvJqsrQFbs/bXWlgHTlU9u9mEWpxto+nIFEEsKdvXPfuqLkqqM8LL9sK
dmA6YTk0V05COKpt+jKWjfq7ljOCIbTm7/rHZ+8hsMBfPZ3eQdbmtCFyy+BaEjdv
HrnhSKh74Y9gh4vooat+E4u0rTa43nsLnpamzP/WZe/WTW184FoD77MmDuFurgzA
VED45w10qOS0bTGDxC2bjKc7ULmztjn8s/WHMZQiH/c1T3rnUbYl2tZC3lWf3DYu
0CXYzDW5AgMBAAECggEAAPU5xFLYIvIl/C6H44D9TA24gDNmtN85cUc8EdeM8ohT
1p3n364euIO6mMaElUVwp+B92nB44gBb6/HKO6qpawydpFfpxhZgVyjW1IxyaMk1
EZh/ANrsNP91CfuMz5UMCFDmG5ZYyyN18l5nSv/qKr2al0ZPkozYHFPnzux2s2mS
XXAfExcAnNkIv0r7sdsgo5o8MukgwbGeZ86o3Jiaka1efSzZsy6hcWt8AGtwfTh5
OtTKBNRt/z3UiOcg+HrtJ/FMGdVQsqMGvueyngGkr/rYiK7S8hptrkTTjQ76btAt
zBI0RQEls8xtGOs2GiG6ubWEigJY6Hx5M+39iRgGlQKBgQD2FN2y03YmLjszlwpB
1r1ZcMzB/JUw5fDXe9Z9/HYlMco9ilyskgJxTYq6qWngf2sAB+1pZn0jIWIf7GHc
dxK77hLXwkPdqBPYZS6/nM4AdbibgavUOSR7EaAgVT4jr5EigJOxmUYTH7pdr440
3Zq8o+r7LcQr8V0OTnQfCJftbQKBgQDaQ2nS89LZMbQSGl/+SD1OX5Pbg7h7hv+A
ZwUAc9Ca/gYwSqLBu5mdNRBRYTLnEsTqqo04EFpysrS+G7hajxqRa1FHa41zDQ+d
F3pSDNo0EZd/i0rQsEFRsUFG6P48wKmFyLitkhSHLEUqXz3UmPG1V4/0trvqNSCx
kzsizBs1/QKBgQCZQgWRCgHbZY+ZYcgRmRv0SDw91IFWIt8MVSQQ8trh71B1Y2a0
U3sR9akg98Ho/3I0YruJmTr2ViQ2nZGVLNOOF4fEuEhsE/HII7wpug7SWn7O2sOZ
OL1vqFqByJUaxI0vX8ScJ0ltP6ViE6QNaLamJbCDHs2+UGQUNOg9K6zzQQKBgC2S
reHyLzBShHrTLv/1LXfT1RecpUSFp4uz9wNlK0VxjPFAZEN3XFfK4KFdXjeJX7xv
6BSwtXIFhl+7gf7GqpF6ivoSpvJC4+O1J0FClb0Rf0SOXQy+AKWCEVMxCwS8Zakd
hBIZ0ld3EuoKAOsHFFD8+33pOctpVG4/g7V8UKIxAoGBAJdPph8sdVLxsg1TRSRM
nimLa1KvwvFtoKjtlY0rAyeQwn2xD/dweXSoPks3Kq2R6kXyJfHKsitG001xb75J
3YwYFtYMZsDqbxzp4l7lD/rfvUY3sKKXuSnoya8MEacSRNDFt0L1ni8sextK3cC0
Cy3zGDE1M9/IzahL6ydf2kg/
-----END PRIVATE KEY-----
";

/// The service-account email the test key signs as.
pub(crate) const CLIENT_EMAIL: &str = "firebase@meno-test.iam.gserviceaccount.com";

/// A service-account JSON whose `token_uri` is `token_uri` and whose private key is
/// [`PRIVATE_KEY_PEM`].
pub(crate) fn service_account_json(token_uri: &str) -> String {
    format!(
        r#"{{
            "type": "service_account",
            "project_id": "meno-test",
            "private_key_id": "test-key-id",
            "private_key": {private_key:?},
            "client_email": {email:?},
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": {token_uri:?}
        }}"#,
        private_key = PRIVATE_KEY_PEM,
        email = CLIENT_EMAIL,
    )
}

/// A service-account JSON with a chosen `private_key`, for the "the key is unusable"
/// tests. `client_email` is valid so only the key can be at fault.
pub(crate) fn service_account_json_with_key(private_key: &str) -> String {
    format!(
        r#"{{
            "private_key": {private_key:?},
            "client_email": {email:?}
        }}"#,
        email = CLIENT_EMAIL,
    )
}
