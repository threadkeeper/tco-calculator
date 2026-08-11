use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::repository::current_timestamp;

pub const CURRENT_PRIVACY_NOTICE_VERSION: &str = "2026-08-11-internal-pilot-v1";
pub(crate) const PRIVACY_CONSENT_DOCUMENT_ID: &str = "privacy-consent";
pub(crate) const PRIVACY_CONSENT_DOCUMENT_TYPE: &str = "privacy_consent";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyConsentProfile {
    pub display_name: Option<String>,
    pub email_address: Option<String>,
    pub allow_contact: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivacyConsentDocument {
    pub id: String,
    pub document_type: String,
    pub owner_id: String,
    pub notice_version: String,
    pub accepted_at: String,
    pub display_name: Option<String>,
    pub email_address: Option<String>,
    pub allow_contact: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PrivacyConsentError {
    #[error("privacy consent record is invalid")]
    InvalidRecord,
    #[error("privacy consent persistence is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait PrivacyConsentRepository: Send + Sync {
    async fn get(
        &self,
        owner_id: &str,
    ) -> Result<Option<PrivacyConsentDocument>, PrivacyConsentError>;

    async fn save(
        &self,
        owner_id: &str,
        profile: PrivacyConsentProfile,
    ) -> Result<PrivacyConsentDocument, PrivacyConsentError>;
}

#[derive(Clone, Default)]
pub struct InMemoryPrivacyConsentRepository {
    records: Arc<RwLock<HashMap<String, PrivacyConsentDocument>>>,
}

impl InMemoryPrivacyConsentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PrivacyConsentRepository for InMemoryPrivacyConsentRepository {
    async fn get(
        &self,
        owner_id: &str,
    ) -> Result<Option<PrivacyConsentDocument>, PrivacyConsentError> {
        let records = self
            .records
            .read()
            .map_err(|_| PrivacyConsentError::Unavailable)?;
        let record = records.get(owner_id).cloned();
        if let Some(record) = &record {
            validate_document(record, owner_id)?;
        }
        Ok(record)
    }

    async fn save(
        &self,
        owner_id: &str,
        profile: PrivacyConsentProfile,
    ) -> Result<PrivacyConsentDocument, PrivacyConsentError> {
        let document = new_document(owner_id, profile)?;
        self.records
            .write()
            .map_err(|_| PrivacyConsentError::Unavailable)?
            .insert(owner_id.to_owned(), document.clone());
        Ok(document)
    }
}

pub(crate) fn new_document(
    owner_id: &str,
    profile: PrivacyConsentProfile,
) -> Result<PrivacyConsentDocument, PrivacyConsentError> {
    if owner_id.is_empty() || (profile.allow_contact && profile.email_address.is_none()) {
        return Err(PrivacyConsentError::InvalidRecord);
    }
    let document = PrivacyConsentDocument {
        id: PRIVACY_CONSENT_DOCUMENT_ID.to_owned(),
        document_type: PRIVACY_CONSENT_DOCUMENT_TYPE.to_owned(),
        owner_id: owner_id.to_owned(),
        notice_version: CURRENT_PRIVACY_NOTICE_VERSION.to_owned(),
        accepted_at: current_timestamp().map_err(|_| PrivacyConsentError::Unavailable)?,
        display_name: profile.display_name,
        email_address: profile
            .allow_contact
            .then_some(profile.email_address)
            .flatten(),
        allow_contact: profile.allow_contact,
    };
    validate_document(&document, owner_id)?;
    Ok(document)
}

pub(crate) fn validate_document(
    document: &PrivacyConsentDocument,
    owner_id: &str,
) -> Result<(), PrivacyConsentError> {
    if document.id != PRIVACY_CONSENT_DOCUMENT_ID
        || document.document_type != PRIVACY_CONSENT_DOCUMENT_TYPE
        || document.owner_id != owner_id
        || document.notice_version.is_empty()
        || document.accepted_at.is_empty()
        || document.allow_contact != document.email_address.is_some()
    {
        return Err(PrivacyConsentError::InvalidRecord);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn consent_is_owner_scoped_and_versioned() {
        let repository = InMemoryPrivacyConsentRepository::new();
        let record = repository
            .save(
                "entra:tenant-a:user-a",
                PrivacyConsentProfile {
                    display_name: Some("Synthetic User".to_owned()),
                    email_address: Some("synthetic@example.com".to_owned()),
                    allow_contact: true,
                },
            )
            .await
            .expect("save consent");

        assert_eq!(record.notice_version, CURRENT_PRIVACY_NOTICE_VERSION);
        assert_eq!(record.display_name.as_deref(), Some("Synthetic User"));
        assert_eq!(
            record.email_address.as_deref(),
            Some("synthetic@example.com")
        );
        assert!(record.allow_contact);
        assert!(
            repository
                .get("entra:tenant-b:user-a")
                .await
                .expect("read other owner")
                .is_none()
        );
    }

    #[tokio::test]
    async fn email_is_not_retained_without_contact_permission() {
        let repository = InMemoryPrivacyConsentRepository::new();
        let record = repository
            .save(
                "entra:tenant-a:user-a",
                PrivacyConsentProfile {
                    display_name: None,
                    email_address: Some("discard@example.com".to_owned()),
                    allow_contact: false,
                },
            )
            .await
            .expect("save consent");

        assert_eq!(record.email_address, None);
        assert!(!record.allow_contact);
    }
}
