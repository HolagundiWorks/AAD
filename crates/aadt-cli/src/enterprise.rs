//! M14 enterprise commands: `PERMS`, `CONNECTOR`, `SITE` — registered onto the
//! same interpreter the local API feeds, so HTTP callers inherit the gates.

use aadt_command::{Command, Context};
use aadt_object::{list_object_acl, set_object_acl, ObjectAccess, ObjectId, Role};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::commands::{self, ObjState};
use crate::site_ingest;

pub type ObjStateHandle = Rc<RefCell<ObjState>>;
pub type SharedCorpusHandle = commands::SharedCorpus;

/// `PERMS` — principal / ACL surface (M14.2).
pub struct PermsCmd {
    pub st: ObjStateHandle,
}

impl Command for PermsCmd {
    fn name(&self) -> &str {
        "PERMS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PERMISSIONS", "ACL"]
    }
    fn help(&self) -> &str {
        "PERMS WHOAMI|SHOW [id]|GRANT principal r|rw id|REVOKE principal id — object ACL + identity"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        st.sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "WHOAMI".into());
        match sub.as_str() {
            "WHOAMI" | "ME" | "CAPS" => {
                let user = aadt_object::current_user();
                let signing = if st.audit.is_signing() {
                    "signing=on"
                } else {
                    "signing=off (set AADT_AUDIT_KEY)"
                };
                Ok(format!(
                    "PERMS: principal={user} role=local {signing}; use API --identity/--role for token caps"
                ))
            }
            "SHOW" => {
                if let Some(raw) = args.get(1) {
                    let id = ObjectId(
                        raw.parse::<u64>()
                            .map_err(|_| format!("PERMS SHOW: bad object id {raw:?}"))?,
                    );
                    let Some(o) = st.objects.get(id) else {
                        return Err(format!("PERMS SHOW: no object #{}", id.0));
                    };
                    let acls = list_object_acl(o);
                    if acls.is_empty() {
                        return Ok(format!(
                            "PERMS: object #{} has no ACL props (capability set applies)",
                            id.0
                        ));
                    }
                    let lines: Vec<String> = acls
                        .iter()
                        .map(|(p, a)| format!("  {p} = {}", a.as_str()))
                        .collect();
                    Ok(format!("PERMS object #{}:\n{}", id.0, lines.join("\n")))
                } else {
                    let mut lines = Vec::new();
                    for o in st.objects.iter() {
                        let acls = list_object_acl(o);
                        if acls.is_empty() {
                            continue;
                        }
                        let parts: Vec<String> = acls
                            .iter()
                            .map(|(p, a)| format!("{p}:{}", a.as_str()))
                            .collect();
                        lines.push(format!("  #{} {}", o.id.0, parts.join(" ")));
                    }
                    if lines.is_empty() {
                        Ok("PERMS: no object ACL entries yet".into())
                    } else {
                        Ok(format!("PERMS ACL summary:\n{}", lines.join("\n")))
                    }
                }
            }
            "GRANT" => {
                let principal = args
                    .get(1)
                    .ok_or_else(|| "PERMS GRANT: need principal".to_string())?
                    .clone();
                let access = ObjectAccess::parse(
                    args.get(2)
                        .ok_or_else(|| "PERMS GRANT: need r|rw".to_string())?,
                )
                .ok_or_else(|| "PERMS GRANT: access must be r, rw, or -".to_string())?;
                let id = ObjectId(
                    args.get(3)
                        .ok_or_else(|| "PERMS GRANT: need object id".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "PERMS GRANT: bad object id".to_string())?,
                );
                let Some(o) = st.objects.get_mut(id) else {
                    return Err(format!("PERMS GRANT: no object #{}", id.0));
                };
                let key = set_object_acl(o, &principal, access);
                let summary = format!("{}={}", principal, access.as_str());
                st.audit.record(
                    "perms.grant",
                    id,
                    "",
                    &summary,
                    false,
                    true,
                    &format!("set {key}"),
                );
                st.persist();
                Ok(format!("PERMS: granted {summary} on object #{}", id.0))
            }
            "REVOKE" => {
                let principal = args
                    .get(1)
                    .ok_or_else(|| "PERMS REVOKE: need principal".to_string())?
                    .clone();
                let id = ObjectId(
                    args.get(2)
                        .ok_or_else(|| "PERMS REVOKE: need object id".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "PERMS REVOKE: bad object id".to_string())?,
                );
                let Some(o) = st.objects.get_mut(id) else {
                    return Err(format!("PERMS REVOKE: no object #{}", id.0));
                };
                set_object_acl(o, &principal, ObjectAccess::None);
                st.audit.record(
                    "perms.revoke",
                    id,
                    &principal,
                    "",
                    false,
                    true,
                    "cleared acl",
                );
                st.persist();
                Ok(format!(
                    "PERMS: revoked ACL for {principal} on object #{}",
                    id.0
                ))
            }
            other => Err(format!(
                "PERMS: unknown subcommand {other:?} (WHOAMI|SHOW|GRANT|REVOKE)"
            )),
        }
    }
}

/// `CONNECTOR` — enterprise plugin commands over the command line (M14.4).
pub struct ConnectorCmd {
    pub st: ObjStateHandle,
}

impl Command for ConnectorCmd {
    fn name(&self) -> &str {
        "CONNECTOR"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PLUGIN"]
    }
    fn help(&self) -> &str {
        "CONNECTOR LIST|ERP PUSH BOQ [path]|RFI EXPORT [path]|DMS PUSH [path] — enterprise payloads via command line"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        st.sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "LIST".into());
        match sub.as_str() {
            "LIST" => Ok(
                "CONNECTORS:\n  erp   — PUSH BOQ [out.csv]\n  rfi   — EXPORT [out.txt]\n  dms   — PUSH [out.json]\n  site  — use SITE INDEX (M14.5)"
                    .into(),
            ),
            "ERP" => {
                let action = args
                    .get(1)
                    .map(|s| s.to_ascii_uppercase())
                    .unwrap_or_default();
                if action != "PUSH" {
                    return Err("CONNECTOR ERP: use PUSH BOQ [path]".into());
                }
                let kind = args
                    .get(2)
                    .map(|s| s.to_ascii_uppercase())
                    .unwrap_or_else(|| "BOQ".into());
                if kind != "BOQ" {
                    return Err("CONNECTOR ERP PUSH: only BOQ is implemented".into());
                }
                let out = args
                    .get(3)
                    .cloned()
                    .unwrap_or_else(|| default_out(ctx, "boq-export.csv"));
                let run = aadt_takeoff::run(&ctx.doc, &aadt_takeoff::TakeoffConfig::default());
                if run.boq.is_empty() {
                    return Err(
                        "CONNECTOR ERP: nothing measured — promote objects / draw walls first"
                            .into(),
                    );
                }
                let mut csv = String::from("cost_code,description,quantity,unit\n");
                for line in &run.boq.lines {
                    csv.push_str(&format!(
                        "{},{},{},{}\n",
                        csv_escape(line.cost_code.code),
                        csv_escape(line.description),
                        line.quantity,
                        csv_escape(line.unit.as_str())
                    ));
                }
                std::fs::write(&out, csv).map_err(|e| format!("CONNECTOR ERP: write {out}: {e}"))?;
                st.audit.record(
                    "connector.erp.push",
                    ObjectId(0),
                    "",
                    &out,
                    false,
                    true,
                    "BOQ CSV",
                );
                st.persist();
                Ok(format!(
                    "CONNECTOR ERP: wrote {} BOQ line(s) to {out}",
                    run.boq.len()
                ))
            }
            "RFI" => {
                let action = args
                    .get(1)
                    .map(|s| s.to_ascii_uppercase())
                    .unwrap_or_else(|| "EXPORT".into());
                if action != "EXPORT" {
                    return Err("CONNECTOR RFI: use EXPORT [path]".into());
                }
                let out = args
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| default_out(ctx, "rfi-export.txt"));
                let mut body = String::from("# AADT RFI export\n");
                body.push_str(&format!(
                    "drawing: {}\n",
                    ctx.drawing_path.as_deref().unwrap_or("(unsaved)")
                ));
                body.push_str(&format!("selection: {} entit(y/ies)\n", ctx.selection.len()));
                for id in &ctx.selection {
                    body.push_str(&format!("  entity #{}\n", id.0));
                }
                for o in st.objects.iter().take(50) {
                    body.push_str(&format!("  object #{} {}\n", o.id.0, o.kind));
                }
                std::fs::write(&out, body).map_err(|e| format!("CONNECTOR RFI: write {out}: {e}"))?;
                st.audit.record(
                    "connector.rfi.export",
                    ObjectId(0),
                    "",
                    &out,
                    false,
                    true,
                    "selection+objects",
                );
                st.persist();
                Ok(format!("CONNECTOR RFI: wrote export to {out}"))
            }
            "DMS" => {
                let action = args
                    .get(1)
                    .map(|s| s.to_ascii_uppercase())
                    .unwrap_or_else(|| "PUSH".into());
                if action != "PUSH" {
                    return Err("CONNECTOR DMS: use PUSH [path]".into());
                }
                let out = args
                    .get(2)
                    .cloned()
                    .unwrap_or_else(|| default_out(ctx, "dms-meta.json"));
                let path = ctx.drawing_path.as_deref().unwrap_or("");
                let json = format!(
                    "{{\n  \"source\": \"aadt\",\n  \"drawing\": {},\n  \"entities\": {},\n  \"objects\": {},\n  \"revision\": {}\n}}\n",
                    json_str(path),
                    ctx.doc.len(),
                    st.objects.len(),
                    ctx.doc.revision()
                );
                std::fs::write(&out, json).map_err(|e| format!("CONNECTOR DMS: write {out}: {e}"))?;
                st.audit.record(
                    "connector.dms.push",
                    ObjectId(0),
                    "",
                    &out,
                    false,
                    true,
                    "metadata",
                );
                st.persist();
                Ok(format!("CONNECTOR DMS: wrote metadata to {out}"))
            }
            other => Err(format!(
                "CONNECTOR: unknown {other:?} (LIST|ERP|RFI|DMS)"
            )),
        }
    }
}

/// `SITE` — ingest photos / reports into the corpus (M14.5).
pub struct SiteCmd {
    pub corpus: SharedCorpusHandle,
}

impl Command for SiteCmd {
    fn name(&self) -> &str {
        "SITE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["SITEINDEX"]
    }
    fn help(&self) -> &str {
        "SITE INDEX PHOTO|PDF|REPORT <path> [--caption text] — ingest site data into the local corpus"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        commands::askdocs_sync(&self.corpus, ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        if sub != "INDEX" && sub != "INGEST" {
            return Err("SITE: use INDEX PHOTO|PDF|REPORT <path> [--caption …]".into());
        }
        let kind = args
            .get(1)
            .map(|s| s.to_ascii_uppercase())
            .ok_or_else(|| "SITE INDEX: need PHOTO|PDF|REPORT".to_string())?;
        let path = args
            .get(2)
            .cloned()
            .ok_or_else(|| "SITE INDEX: need a path".to_string())?;
        let caption = caption_from_args(args);
        let text = match kind.as_str() {
            "PHOTO" | "IMAGE" | "IMG" => site_ingest::photo_text(&path, caption.as_deref()),
            "PDF" | "REPORT" | "DOC" => {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("SITE INDEX: cannot read {path}: {e}"))?;
                site_ingest::report_text(&path, &bytes)
            }
            other => {
                return Err(format!(
                    "SITE INDEX: unknown kind {other:?} (PHOTO|PDF|REPORT)"
                ))
            }
        };
        let source = site_ingest::source_label(&kind.to_ascii_lowercase(), &path);
        let added = commands::askdocs_index_text(&self.corpus, &source, &text);
        if added == 0 {
            return Ok(format!("SITE: {path} produced no indexable passages"));
        }
        Ok(format!(
            "SITE: indexed {added} passage(s) from {kind} {path} as {source}"
        ))
    }
}

fn caption_from_args(args: &[String]) -> Option<String> {
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--caption" || args[i] == "-c" {
            if let Some(c) = args.get(i + 1) {
                out = Some(c.clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn default_out(ctx: &Context, name: &str) -> String {
    if let Some(p) = ctx.drawing_path.as_ref() {
        if let Some(parent) = Path::new(p).parent() {
            return parent.join(name).to_string_lossy().into_owned();
        }
    }
    name.into()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Re-export Role parse for `aadt serve --role`.
pub fn parse_role(s: &str) -> Option<Role> {
    Role::parse(s)
}
