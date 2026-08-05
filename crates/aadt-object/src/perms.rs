//! Object-level permissions (M14.2).
//!
//! A **principal** carries a capability set that gates the local API and
//! mutating commands. Per-object ACLs live as `acl.<principal>` props on an
//! [`EngObject`] (`r` / `rw` / `-`) so they ride the existing Object DB
//! persistence without a schema bump. There is no REST ACL API — grant and
//! revoke go through the `PERMS` command line, the same door as every other write.

use crate::EngObject;

/// Who is acting — an API token identity or the local OS user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: String,
}

impl Principal {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    pub fn local() -> Self {
        Self::new(crate::current_user())
    }
}

/// Capability bits for the API / command gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Capability {
    /// Read routes and LIST/INFO queries.
    Read = 1 << 0,
    /// `POST /v1/command` for ordinary drafting / assist lines.
    Command = 1 << 1,
    /// Promote / mutate objects (`OBJECTS RUN`, ACL writes).
    ObjectWrite = 1 << 2,
    /// Read the audit log (`OBJECTS AUDIT`).
    AuditRead = 1 << 3,
    /// Enterprise connector commands (`CONNECTOR …`).
    Connector = 1 << 4,
    /// Grant/revoke, shutdown, set signing key.
    Admin = 1 << 5,
}

/// A bit-set of [`Capability`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self(
            Capability::Read as u32
                | Capability::Command as u32
                | Capability::ObjectWrite as u32
                | Capability::AuditRead as u32
                | Capability::Connector as u32
                | Capability::Admin as u32,
        )
    }

    pub fn has(self, c: Capability) -> bool {
        self.0 & (c as u32) != 0 || self.0 & (Capability::Admin as u32) != 0
    }

    pub fn insert(&mut self, c: Capability) {
        self.0 |= c as u32;
    }

    pub fn remove(&mut self, c: Capability) {
        self.0 &= !(c as u32);
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn names(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (c, name) in [
            (Capability::Read, "read"),
            (Capability::Command, "command"),
            (Capability::ObjectWrite, "object_write"),
            (Capability::AuditRead, "audit_read"),
            (Capability::Connector, "connector"),
            (Capability::Admin, "admin"),
        ] {
            if self.0 & (c as u32) != 0 {
                out.push(name);
            }
        }
        out
    }
}

/// Named roles mapped onto capability sets (CLI `--role` / API config).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    Editor,
    Connector,
    Admin,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "viewer" | "view" | "ro" => Some(Self::Viewer),
            "editor" | "edit" | "rw" => Some(Self::Editor),
            "connector" | "plugin" => Some(Self::Connector),
            "admin" | "root" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
            Self::Connector => "connector",
            Self::Admin => "admin",
        }
    }

    pub fn capabilities(self) -> CapabilitySet {
        let mut c = CapabilitySet::empty();
        match self {
            Self::Viewer => {
                c.insert(Capability::Read);
                c.insert(Capability::AuditRead);
            }
            Self::Editor => {
                c.insert(Capability::Read);
                c.insert(Capability::Command);
                c.insert(Capability::ObjectWrite);
                c.insert(Capability::AuditRead);
            }
            Self::Connector => {
                c.insert(Capability::Read);
                c.insert(Capability::Command);
                c.insert(Capability::Connector);
                c.insert(Capability::AuditRead);
            }
            Self::Admin => return CapabilitySet::all(),
        }
        c
    }
}

/// Access level stored on an object as `acl.<principal>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectAccess {
    /// Explicit deny.
    None,
    /// Read object metadata / select members.
    Read,
    /// Read + mutate props / include in connector export.
    ReadWrite,
}

impl ObjectAccess {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "-" | "none" | "deny" => Some(Self::None),
            "r" | "read" => Some(Self::Read),
            "rw" | "write" | "readwrite" => Some(Self::ReadWrite),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "-",
            Self::Read => "r",
            Self::ReadWrite => "rw",
        }
    }
}

fn acl_key(principal: &str) -> String {
    format!("acl.{}", principal.trim().to_ascii_lowercase())
}

/// Read the ACL entry for `principal` on an object, if any.
pub fn object_acl(obj: &EngObject, principal: &str) -> Option<ObjectAccess> {
    obj.prop(&acl_key(principal))
        .and_then(ObjectAccess::parse)
}

/// Set an ACL entry. [`ObjectAccess::None`] stores an explicit deny (`-`).
pub fn set_object_acl(obj: &mut EngObject, principal: &str, access: ObjectAccess) -> String {
    let key = acl_key(principal);
    obj.props.insert(key.clone(), access.as_str().into());
    key
}

/// Remove an ACL prop entirely (fall back to the principal's capability set).
pub fn clear_object_acl(obj: &mut EngObject, principal: &str) {
    obj.props.remove(&acl_key(principal));
}

/// List all `acl.*` entries on an object.
pub fn list_object_acl(obj: &EngObject) -> Vec<(String, ObjectAccess)> {
    obj.props
        .iter()
        .filter_map(|(k, v)| {
            let principal = k.strip_prefix("acl.")?;
            let access = ObjectAccess::parse(v)?;
            Some((principal.to_string(), access))
        })
        .collect()
}

/// Does `principal` with `caps` have at least `need` on this object?
///
/// No ACL prop → fall back to the principal's capability set (`ObjectWrite` for
/// write, `Read` for read). An explicit `-` denies even an admin at the object
/// layer only if we checked ACL first — admins still bypass via capability.
pub fn allows_object(
    obj: &EngObject,
    principal: &str,
    caps: CapabilitySet,
    need: ObjectAccess,
) -> bool {
    if caps.has(Capability::Admin) {
        return true;
    }
    match object_acl(obj, principal) {
        Some(ObjectAccess::None) => false,
        Some(ObjectAccess::Read) => matches!(need, ObjectAccess::Read),
        Some(ObjectAccess::ReadWrite) => true,
        None => match need {
            ObjectAccess::None => true,
            ObjectAccess::Read => caps.has(Capability::Read),
            ObjectAccess::ReadWrite => caps.has(Capability::ObjectWrite),
        },
    }
}

/// Classify a command line into the capability required to run it.
///
/// Returns `None` for empty / comment-like lines (still need `Command` at the
/// HTTP gate — callers decide).
pub fn command_capability(line: &str) -> Capability {
    let first = line
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('\'')
        .to_ascii_uppercase();
    match first.as_str() {
        "PERMS" => {
            let sub = line
                .trim()
                .split_whitespace()
                .nth(1)
                .unwrap_or("WHOAMI")
                .to_ascii_uppercase();
            match sub.as_str() {
                "SHOW" | "WHOAMI" | "CAPS" => Capability::Read,
                _ => Capability::Admin,
            }
        }
        "CONNECTOR" | "ERP" | "RFI" | "DMS" => Capability::Connector,
        "OBJECTS" => {
            // RUN needs ObjectWrite; LIST/INFO need Read; AUDIT needs AuditRead —
            // the command itself re-checks. Gate at ObjectWrite so viewers can't
            // promote via HTTP even if they somehow got Command.
            let sub = line
                .trim()
                .split_whitespace()
                .nth(1)
                .unwrap_or("RUN")
                .to_ascii_uppercase();
            match sub.as_str() {
                "LIST" | "INFO" => Capability::Read,
                "AUDIT" => Capability::AuditRead,
                _ => Capability::ObjectWrite,
            }
        }
        "SHUTDOWN" => Capability::Admin,
        "SITE" => Capability::Command, // indexing is a corpus write via command
        _ => Capability::Command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngObject, ObjectId, ObjectKind};
    use std::collections::BTreeMap;

    fn wall() -> EngObject {
        EngObject {
            id: ObjectId(1),
            kind: ObjectKind::Wall,
            entities: vec![],
            props: BTreeMap::new(),
            confidence: 1.0,
            revision: 1,
        }
    }

    #[test]
    fn role_caps_nest() {
        assert!(Role::Admin.capabilities().has(Capability::Connector));
        assert!(!Role::Viewer.capabilities().has(Capability::Command));
        assert!(Role::Editor.capabilities().has(Capability::ObjectWrite));
        assert!(Role::Connector.capabilities().has(Capability::Connector));
    }

    #[test]
    fn acl_grant_and_deny() {
        let mut o = wall();
        set_object_acl(&mut o, "alice", ObjectAccess::Read);
        assert_eq!(object_acl(&o, "alice"), Some(ObjectAccess::Read));
        let caps = Role::Viewer.capabilities();
        assert!(allows_object(&o, "alice", caps, ObjectAccess::Read));
        assert!(!allows_object(&o, "alice", caps, ObjectAccess::ReadWrite));
        set_object_acl(&mut o, "bob", ObjectAccess::None);
        assert!(!allows_object(
            &o,
            "bob",
            Role::Editor.capabilities(),
            ObjectAccess::Read
        ));
    }

    #[test]
    fn command_capability_classifies() {
        assert_eq!(
            command_capability("OBJECTS RUN"),
            Capability::ObjectWrite
        );
        assert_eq!(command_capability("OBJECTS LIST"), Capability::Read);
        assert_eq!(command_capability("OBJECTS AUDIT"), Capability::AuditRead);
        assert_eq!(command_capability("CONNECTOR ERP PUSH"), Capability::Connector);
        assert_eq!(command_capability("LINE 0,0 1,0"), Capability::Command);
    }
}
