pub mod infrai_client;

use infrai_client::{InfraiClient, InfraiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct DeveloperSignup {
    pub request_id: String,
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub signed_up_with: Channel,
    pub build_event: BuildEvent,
    pub release_operation: ReleaseOperation,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Email,
    Sms,
}

#[derive(Debug, Deserialize)]
pub struct BuildEvent {
    pub repository: String,
    pub revision: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseOperation {
    pub environment: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct Diagnostic {
    pub trace_id: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct OnboardingReceipt {
    pub user_id: String,
    pub channel: Channel,
    pub message_id: String,
    pub trace_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OnboardingError {
    #[error("onboarding consent is required")]
    ConsentRequired,
    #[error("neither signup channel can receive the welcome")]
    NoDeliverableChannel,
    #[error(transparent)]
    Infrai(#[from] InfraiError),
}

pub async fn onboard(
    client: &InfraiClient,
    signup: DeveloperSignup,
) -> Result<OnboardingReceipt, OnboardingError> {
    if !client.has_onboarding_consent(&signup.user_id).await? {
        return Err(OnboardingError::ConsentRequired);
    }

    let email_allowed = !client.email_suppressed(&signup.email).await?;
    let sms_allowed = !client.sms_suppressed(&signup.phone).await?;
    let channel = select_channel(signup.signed_up_with, email_allowed, sms_allowed)
        .ok_or(OnboardingError::NoDeliverableChannel)?;
    let text = format!(
        "Welcome, {}. Build {} at {} is linked to release {} in {}. Diagnostic trace: {} ({}).",
        signup.name,
        signup.build_event.revision,
        signup.build_event.repository,
        signup.release_operation.version,
        signup.release_operation.environment,
        signup.diagnostic.trace_id,
        signup.diagnostic.source
    );
    let message_id = match channel {
        Channel::Email => {
            client
                .send_email(&signup.email, &text, &signup.request_id)
                .await?
        }
        Channel::Sms => {
            client
                .send_sms(&signup.phone, &text, &signup.request_id)
                .await?
        }
    };
    Ok(OnboardingReceipt {
        user_id: signup.user_id,
        channel,
        message_id,
        trace_id: signup.diagnostic.trace_id,
    })
}

pub fn select_channel(
    preferred: Channel,
    email_allowed: bool,
    sms_allowed: bool,
) -> Option<Channel> {
    match (preferred, email_allowed, sms_allowed) {
        (Channel::Email, true, _) => Some(Channel::Email),
        (Channel::Sms, _, true) => Some(Channel::Sms),
        (Channel::Email, false, true) => Some(Channel::Sms),
        (Channel::Sms, true, false) => Some(Channel::Email),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppressed_email_hands_welcome_to_sms() {
        assert_eq!(
            select_channel(Channel::Email, false, true),
            Some(Channel::Sms)
        );
    }
}
