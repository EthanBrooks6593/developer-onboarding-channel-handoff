use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use developer_onboarding_handoff::{
    infrai_client::InfraiClient, onboard, DeveloperSignup, OnboardingError,
};
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = InfraiClient::from_env().expect("set INFRAI_API_KEY");
    let app = Router::new()
        .route("/onboard", post(handle_onboard))
        .with_state(client);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("bind port 8080");
    println!("developer onboarding listening on http://127.0.0.1:8080");
    axum::serve(listener, app)
        .await
        .expect("serve onboarding API");
}

async fn handle_onboard(
    State(client): State<InfraiClient>,
    Json(signup): Json<DeveloperSignup>,
) -> impl IntoResponse {
    match onboard(&client, signup).await {
        Ok(receipt) => (StatusCode::OK, Json(json!(receipt))),
        Err(error) => {
            let status = match &error {
                OnboardingError::ConsentRequired | OnboardingError::NoDeliverableChannel => {
                    StatusCode::UNPROCESSABLE_ENTITY
                }
                OnboardingError::Infrai(inner) => inner.caller_status(),
            };
            (status, Json(json!({ "error": error.to_string() })))
        }
    }
}
