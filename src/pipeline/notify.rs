//! Shared bridge-echo notification helpers.
//!
//! Used by both Twilio and Discord stream handlers to notify bridge-echo
//! of session lifecycle events for cross-channel routing.

/// Notify bridge-echo that a voice session started so it can pre-register
/// for cross-channel routing before any voice utterance flows through.
pub async fn notify_session_started(
    bridge_url: &str,
    call_sid: &str,
    sender: &str,
    transport: &str,
) {
    let url = format!("{}/session-started", bridge_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    match client
        .post(&url)
        .json(&serde_json::json!({
            "call_sid": call_sid,
            "sender": sender,
            "transport": transport,
        }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(call_sid, "Notified bridge-echo of session start");
        }
        Ok(resp) => {
            tracing::warn!(
                call_sid,
                status = %resp.status(),
                "bridge-echo session-started notification returned error"
            );
        }
        Err(e) => {
            tracing::warn!(
                call_sid,
                "Failed to notify bridge-echo of session start: {e}"
            );
        }
    }
}

/// Notify bridge-echo that a voice session ended so it stops routing
/// cross-channel responses to voice.
pub async fn notify_call_ended(bridge_url: &str, call_sid: &str) {
    let url = format!("{}/call-ended", bridge_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    match client
        .post(&url)
        .json(&serde_json::json!({ "call_sid": call_sid }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(call_sid, "Notified bridge-echo of session end");
        }
        Ok(resp) => {
            tracing::warn!(
                call_sid,
                status = %resp.status(),
                "bridge-echo call-ended notification returned error"
            );
        }
        Err(e) => {
            tracing::warn!(call_sid, "Failed to notify bridge-echo of session end: {e}");
        }
    }
}
