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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    pub tenant_id: Uuid,
    pub object_id: Uuid,
    pub display_name: Option<String>,
    pub email_address: Option<String>,
}

impl Principal {
    pub fn owner_id(&self) -> String {
        format!("entra:{}:{}", self.tenant_id, self.object_id)
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

    Ok(Principal {
        tenant_id,
        object_id,
        display_name,
        email_address,
    })
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
