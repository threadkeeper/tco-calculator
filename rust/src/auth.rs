use axum::http::{HeaderMap, HeaderValue};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

use crate::config::{AppEnvironment, Config, LocalAuthSettings};

pub const CLIENT_PRINCIPAL_HEADER: &str = "x-ms-client-principal";
const MAX_PRINCIPAL_HEADER_BYTES: usize = 16_384;
const TENANT_ID_CLAIM: &str = "http://schemas.microsoft.com/identity/claims/tenantid";
const OBJECT_ID_CLAIM: &str = "http://schemas.microsoft.com/identity/claims/objectidentifier";
const DISPLAY_NAME_CLAIM: &str = "name";
const DISPLAY_NAME_URI_CLAIM: &str = "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/name";
const EMAIL_CLAIM: &str = "email";
const EMAIL_URI_CLAIM: &str = "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress";
const PREFERRED_USERNAME_CLAIM: &str = "preferred_username";
const SCOPE_CLAIM: &str = "scp";
const SCOPE_URI_CLAIM: &str = "http://schemas.microsoft.com/identity/claims/scope";
const AUTHORIZED_PARTY_CLAIM: &str = "azp";
const APPLICATION_ID_CLAIM: &str = "appid";
const TOKEN_TYPE_CLAIM: &str = "idtyp";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    pub tenant_id: Uuid,
    pub object_id: Uuid,
    pub display_name: Option<String>,
    pub email_address: Option<String>,
    companion_access: Option<CompanionAccessClaims>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompanionAccessClaims {
    authorized_party: Uuid,
    scopes: Vec<String>,
    app_only: bool,
}

impl Principal {
    pub fn owner_id(&self) -> String {
        format!("entra:{}:{}", self.tenant_id, self.object_id)
    }

    pub fn authorize_companion(
        &self,
        authorized_party: Uuid,
        required_scope: &str,
    ) -> Result<(), AuthError> {
        let access = self
            .companion_access
            .as_ref()
            .ok_or(AuthError::MissingDelegatedAccess)?;
        if access.app_only
            || access.authorized_party != authorized_party
            || !access.scopes.iter().any(|scope| scope == required_scope)
        {
            return Err(AuthError::CompanionAccessDenied);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AuthError {
    #[error("platform principal header is malformed")]
    MalformedPrincipal,
    #[error("platform principal is missing tenant or object identity claims")]
    MissingIdentityClaims,
    #[error("platform principal contains ambiguous identity claims")]
    AmbiguousIdentityClaims,
    #[error("platform principal is missing delegated access claims")]
    MissingDelegatedAccess,
    #[error("platform principal is not authorized as the Calculator companion")]
    CompanionAccessDenied,
}

#[derive(Deserialize)]
struct PlatformPrincipal {
    claims: Vec<PlatformClaim>,
}

#[derive(Deserialize)]
struct PlatformClaim {
    typ: String,
    val: String,
}

pub fn resolve_principal(
    headers: &HeaderMap,
    config: &Config,
) -> Result<Option<Principal>, AuthError> {
    if config.environment == AppEnvironment::Local {
        return Ok(config.local_auth.as_ref().map(Principal::from));
    }

    headers
        .get(CLIENT_PRINCIPAL_HEADER)
        .map(parse_platform_principal)
        .transpose()
}

pub fn parse_platform_principal(header: &HeaderValue) -> Result<Principal, AuthError> {
    let encoded = header.to_str().map_err(|_| AuthError::MalformedPrincipal)?;
    if encoded.is_empty() || encoded.len() > MAX_PRINCIPAL_HEADER_BYTES {
        return Err(AuthError::MalformedPrincipal);
    }
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| AuthError::MalformedPrincipal)?;
    if decoded.len() > MAX_PRINCIPAL_HEADER_BYTES {
        return Err(AuthError::MalformedPrincipal);
    }
    let platform: PlatformPrincipal =
        serde_json::from_slice(&decoded).map_err(|_| AuthError::MalformedPrincipal)?;

    let tenant_id = unique_claim(&platform.claims, &["tid", TENANT_ID_CLAIM])?
        .ok_or(AuthError::MissingIdentityClaims)
        .and_then(parse_identity_id)?;
    let object_id = unique_claim(&platform.claims, &["oid", OBJECT_ID_CLAIM])?
        .ok_or(AuthError::MissingIdentityClaims)
        .and_then(parse_identity_id)?;
    let display_name = unique_claim(
        &platform.claims,
        &[DISPLAY_NAME_CLAIM, DISPLAY_NAME_URI_CLAIM],
    )?
    .filter(|value| {
        let length = value.chars().count();
        (1..=200).contains(&length) && !value.chars().any(char::is_control)
    })
    .map(str::to_owned);
    let email_address = optional_email_claim(&platform.claims);
    let companion_access = companion_access_claims(&platform.claims)?;

    Ok(Principal {
        tenant_id,
        object_id,
        display_name,
        email_address,
        companion_access,
    })
}

fn companion_access_claims(
    claims: &[PlatformClaim],
) -> Result<Option<CompanionAccessClaims>, AuthError> {
    let scope = unique_claim(claims, &[SCOPE_CLAIM, SCOPE_URI_CLAIM])?;
    let authorized_party = unique_claim(claims, &[AUTHORIZED_PARTY_CLAIM, APPLICATION_ID_CLAIM])?;
    if scope.is_none() && authorized_party.is_none() {
        return Ok(None);
    }
    let scope = scope.ok_or(AuthError::MissingDelegatedAccess)?;
    let authorized_party = authorized_party
        .ok_or(AuthError::MissingDelegatedAccess)
        .and_then(parse_identity_id)?;
    let scopes = scope
        .split_ascii_whitespace()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if scopes.is_empty()
        || scopes.len() > 32
        || scopes.iter().any(|value| {
            value.len() > 200
                || !value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/')
                })
        })
    {
        return Err(AuthError::MalformedPrincipal);
    }
    let app_only = unique_claim(claims, &[TOKEN_TYPE_CLAIM])?
        .is_some_and(|value| value.eq_ignore_ascii_case("app"));
    Ok(Some(CompanionAccessClaims {
        authorized_party,
        scopes,
        app_only,
    }))
}

fn optional_email_claim(claims: &[PlatformClaim]) -> Option<String> {
    for accepted_types in [
        &[EMAIL_CLAIM, EMAIL_URI_CLAIM][..],
        &[PREFERRED_USERNAME_CLAIM][..],
    ] {
        let mut values = claims
            .iter()
            .filter(|claim| accepted_types.contains(&claim.typ.as_str()))
            .map(|claim| claim.val.trim());
        let Some(candidate) = values.next() else {
            continue;
        };
        if values.any(|value| value != candidate) {
            return None;
        }
        if is_email_address(candidate) {
            return Some(candidate.to_owned());
        }
    }
    None
}

pub(crate) fn is_email_address(value: &str) -> bool {
    let length = value.chars().count();
    if !(3..=254).contains(&length)
        || value != value.trim()
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || character == ','
                || character == ';'
        })
    {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty()
        && local.chars().count() <= 64
        && !domain.is_empty()
        && domain.chars().count() <= 253
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && parts.next().is_none()
}

fn unique_claim<'a>(
    claims: &'a [PlatformClaim],
    accepted_types: &[&str],
) -> Result<Option<&'a str>, AuthError> {
    let mut values = claims
        .iter()
        .filter(|claim| accepted_types.contains(&claim.typ.as_str()))
        .map(|claim| claim.val.as_str());
    let value = values.next();
    if values.next().is_some() {
        return Err(AuthError::AmbiguousIdentityClaims);
    }
    Ok(value)
}

fn parse_identity_id(value: &str) -> Result<Uuid, AuthError> {
    Uuid::parse_str(value).map_err(|_| AuthError::MalformedPrincipal)
}

impl From<&LocalAuthSettings> for Principal {
    fn from(settings: &LocalAuthSettings) -> Self {
        Self {
            tenant_id: settings.tenant_id,
            object_id: settings.object_id,
            display_name: Some(settings.display_name.clone()),
            email_address: None,
            companion_access: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_and_object_id_form_the_owner_boundary() {
        let tenant = Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("tenant UUID");
        let object = Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("object UUID");
        let header = encoded_header(&[
            (TENANT_ID_CLAIM, &tenant.to_string()),
            (OBJECT_ID_CLAIM, &object.to_string()),
            (DISPLAY_NAME_CLAIM, "Synthetic User"),
            (PREFERRED_USERNAME_CLAIM, "synthetic@example.com"),
        ]);

        let principal = parse_platform_principal(&header).expect("valid principal");

        assert_eq!(
            principal.owner_id(),
            "entra:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222"
        );
        assert_eq!(principal.display_name.as_deref(), Some("Synthetic User"));
        assert_eq!(
            principal.email_address.as_deref(),
            Some("synthetic@example.com")
        );
    }

    #[test]
    fn duplicate_or_missing_identity_claims_are_rejected() {
        let duplicate = encoded_header(&[
            ("tid", "11111111-1111-1111-1111-111111111111"),
            (TENANT_ID_CLAIM, "11111111-1111-1111-1111-111111111111"),
            ("oid", "22222222-2222-2222-2222-222222222222"),
        ]);
        let missing = encoded_header(&[("tid", "11111111-1111-1111-1111-111111111111")]);

        assert_eq!(
            parse_platform_principal(&duplicate),
            Err(AuthError::AmbiguousIdentityClaims)
        );
        assert_eq!(
            parse_platform_principal(&missing),
            Err(AuthError::MissingIdentityClaims)
        );
    }

    #[test]
    fn companion_authorization_accepts_v1_appid_alias() {
        let client_id =
            Uuid::parse_str("33333333-3333-3333-3333-333333333333").expect("client UUID");
        let header = encoded_header(&[
            ("tid", "11111111-1111-1111-1111-111111111111"),
            ("oid", "22222222-2222-2222-2222-222222222222"),
            (SCOPE_CLAIM, "calculator.launch"),
            (APPLICATION_ID_CLAIM, &client_id.to_string()),
        ]);

        let principal = parse_platform_principal(&header).expect("valid v1 delegated principal");

        assert_eq!(
            principal.authorize_companion(client_id, "calculator.launch"),
            Ok(())
        );
    }

    #[test]
    fn companion_authorization_rejects_ambiguous_client_claims() {
        let header = encoded_header(&[
            ("tid", "11111111-1111-1111-1111-111111111111"),
            ("oid", "22222222-2222-2222-2222-222222222222"),
            (SCOPE_CLAIM, "calculator.launch"),
            (
                AUTHORIZED_PARTY_CLAIM,
                "33333333-3333-3333-3333-333333333333",
            ),
            (APPLICATION_ID_CLAIM, "33333333-3333-3333-3333-333333333333"),
        ]);

        assert_eq!(
            parse_platform_principal(&header),
            Err(AuthError::AmbiguousIdentityClaims)
        );
    }

    #[test]
    fn ambiguous_email_metadata_is_ignored() {
        let header = encoded_header(&[
            ("tid", "11111111-1111-1111-1111-111111111111"),
            ("oid", "22222222-2222-2222-2222-222222222222"),
            (EMAIL_CLAIM, "first@example.com"),
            (EMAIL_URI_CLAIM, "second@example.com"),
        ]);

        let principal = parse_platform_principal(&header).expect("valid identity");

        assert_eq!(principal.email_address, None);
    }

    fn encoded_header(claims: &[(&str, &str)]) -> HeaderValue {
        let claims = claims
            .iter()
            .map(|(typ, val)| serde_json::json!({ "typ": typ, "val": val }))
            .collect::<Vec<_>>();
        let encoded = STANDARD.encode(
            serde_json::to_vec(&serde_json::json!({ "claims": claims }))
                .expect("serialize principal"),
        );
        HeaderValue::from_str(&encoded).expect("valid header")
    }
}
