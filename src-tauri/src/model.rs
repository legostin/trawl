use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlParts {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMessage {
    /// Заголовки в порядке получения (дубликаты сохраняются).
    pub headers: Vec<(String, String)>,
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
    pub body_is_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMessage {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
    pub body_is_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    /// Миллисекунды от старта прокси-сессии; None пока не наступило.
    pub sent: Option<u64>,
    pub ttfb: Option<u64>,
    pub done: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FlowState {
    Pending,
    Completed,
    Error,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub id: u64,
    /// Unix-время в мс, когда запрос перехвачен.
    pub timestamp: u64,
    pub method: String,
    pub url: UrlParts,
    pub request: HttpMessage,
    pub response: Option<ResponseMessage>,
    pub timings: Timings,
    pub state: FlowState,
    /// Заполняется при state == Error.
    pub error: Option<String>,
    /// Имена сработавших правил-скриптов (индикатор «изменён»).
    pub applied_rules: Vec<String>,
    /// Трасса операций правил: {rule, op, path?, nodes?, status?, ms?}.
    #[serde(default)]
    pub rule_trace: Vec<serde_json::Value>,
    /// Set while the flow is held on a breakpoint: "request" | "response".
    #[serde(default)]
    pub paused_phase: Option<String>,
}

impl Flow {
    pub fn new_request(id: u64, method: String, url: UrlParts, request: HttpMessage) -> Flow {
        Flow {
            id,
            timestamp: 0,
            method,
            url,
            request,
            response: None,
            timings: Timings { sent: None, ttfb: None, done: None },
            state: FlowState::Pending,
            error: None,
            applied_rules: Vec::new(),
            rule_trace: Vec::new(),
            paused_phase: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_serializes_to_camel_case_json() {
        let flow = Flow::new_request(
            1,
            "GET".into(),
            UrlParts {
                scheme: "http".into(),
                host: "example.com".into(),
                port: 80,
                path: "/api/v1".into(),
            },
            HttpMessage {
                headers: vec![("Accept".into(), "application/json".into())],
                body: b"".to_vec(),
                body_is_text: true,
            },
        );
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("\"bodyIsText\":true"), "json was: {json}");
        assert!(json.contains("\"state\":\"pending\""), "json was: {json}");

        let back: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 1);
        assert_eq!(back.method, "GET");
    }

    #[test]
    fn flow_paused_phase_defaults_none_and_roundtrips() {
        let mut flow = Flow::new_request(
            1,
            "GET".into(),
            UrlParts { scheme: "http".into(), host: "h".into(), port: 80, path: "/".into() },
            HttpMessage { headers: vec![], body: vec![], body_is_text: true },
        );
        assert!(flow.paused_phase.is_none());
        flow.paused_phase = Some("request".into());
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("\"pausedPhase\":\"request\""), "json was: {json}");
        let back: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.paused_phase.as_deref(), Some("request"));
    }
}
