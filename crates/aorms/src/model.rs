//! What AORMS returns, read defensively from JSON (a missing or odd field is a default, never a panic).
use serde_json::Value;

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capabilities {
    pub write: bool,
    pub fees_manage: bool,
    pub cost_approve: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Studio {
    pub firm_id: String,
    pub name: String,
    pub public_id: Option<String>,
}

/// `POST/GET /session`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionInfo {
    pub session_id: String,
    pub email: Option<String>,
    pub studio: Studio,
    pub role: String,
    pub capabilities: Capabilities,
    pub connected: bool,
    pub plan: Option<String>,
    pub expires_at: Option<String>,
}

impl SessionInfo {
    pub(crate) fn from_json(v: &Value) -> Option<SessionInfo> {
        let session_id = s(v, "sessionId").filter(|x| !x.is_empty())?;
        let st = v.get("studio").cloned().unwrap_or(Value::Null);
        let caps = v.get("capabilities").cloned().unwrap_or(Value::Null);
        let ent = v.get("entitlement").cloned().unwrap_or(Value::Null);
        Some(SessionInfo {
            session_id,
            email: v.get("account").and_then(|a| s(a, "email")),
            studio: Studio { firm_id: s(&st, "firmId").unwrap_or_default(), name: s(&st, "name").unwrap_or_default(), public_id: s(&st, "publicId") },
            role: s(v, "role").unwrap_or_default(),
            capabilities: Capabilities { write: b(&caps, "write"), fees_manage: b(&caps, "fees:manage"), cost_approve: b(&caps, "cost:approve") },
            connected: b(&ent, "connected"),
            plan: s(&ent, "plan"),
            expires_at: s(&ent, "expiresAt"),
        })
    }
}

/// An AQC/AAD binding of an AORMS project (one per project, shared by both apps).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectBinding {
    pub id: String,
    pub head_seq: i64,
    pub edited_by_other: bool,
}

/// One entry of `GET /projects`.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineProject {
    pub project_office_id: String,
    pub reference: String,
    pub title: String,
    pub status: Option<String>,
    pub city: Option<String>,
    pub binding: Option<ProjectBinding>,
}

impl OnlineProject {
    pub(crate) fn from_json(v: &Value) -> Option<OnlineProject> {
        let binding = v.get("aqc").filter(|a| a.is_object()).and_then(|a| {
            Some(ProjectBinding {
                id: s(a, "id")?,
                head_seq: a.get("headSeq").and_then(Value::as_i64).unwrap_or(0),
                edited_by_other: b(a, "editedByOther"),
            })
        });
        Some(OnlineProject {
            project_office_id: s(v, "projectOfficeId")?,
            reference: s(v, "ref").unwrap_or_default(),
            title: s(v, "title").unwrap_or_default(),
            status: s(v, "status"),
            city: s(v, "city"),
            binding,
        })
    }
}
