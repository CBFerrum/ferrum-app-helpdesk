//! A reply on its way to the customer: the email it becomes, and the files it carries.
//!
//! Where the Mail app is installed the reply is a `Communication` on the ticket, so Mail sends it with
//! proper threading headers (In-Reply-To and References from the customer's last message), copies the
//! ticket's CC and keeps it on the ticket's timeline; the customer's answer comes back to Mail and
//! [`crate::inbound`]. Without Mail it goes through the platform's mail (`Kernel::queue_mail`), one
//! HTML message to each address with no threading headers. Either way the subject carries the ticket's
//! number and secret (`[TKT-2026-000123/…]`), which is what matches an answer to its ticket when no header
//! does, and the wording around the agent's words is the message template `helpdesk.reply`, which the
//! workspace edits. Files are the platform's stored files: an agent uploads one (`POST /api/files`),
//! then names it in the message.

use ferrum_domain::util::{b, id, rows, s, sys};
use ferrum_kernel::{Ctx, Doc, Error, Filter, HookCtx, Kernel, Result, Tx};
use serde_json::{Value, json};
use uuid::Uuid;

/// Most files one message carries, and their size together: what one queued email can attach.
pub const MAX_FILES: usize = 10;
pub const MAX_BYTES: i64 = 20 << 20;

/// The longest subject the platform's mail takes.
const MAX_SUBJECT: usize = 300;

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// "Re: [TKT-2026-000123/secret] Printer jam": the subject the template `helpdesk.reply` gives by default. The
/// number and the secret say which ticket it is about, so an answer finds it even when its headers do not.
pub fn subject(number: &str, token: &str, subject: &str) -> String {
    let mut out = format!("Re: [{number}/{token}] ");
    for c in subject.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > MAX_SUBJECT {
            break;
        }
        out.push(c);
    }
    out
}

fn is_web(url: &str) -> bool {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.contains(char::is_whitespace))
}

/// The text of a bullet-list line ("- one", "* one").
fn bullet(line: &str) -> Option<String> {
    let l = line.trim_start();
    l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).map(str::to_string)
}

/// The inline Markdown agents use, in text that is already escaped: `code`, **bold**, [links](https://…) and
/// bare web addresses. Nothing else is turned into markup.
fn inline(text: &str) -> String {
    let e = escape(text);
    let mut out = String::with_capacity(e.len());
    let mut i = 0;
    while i < e.len() {
        let rest = &e[i..];
        if let Some(r) = rest.strip_prefix('`')
            && let Some(end) = r.find('`').filter(|&n| n > 0)
        {
            out.push_str(&format!("<code>{}</code>", &r[..end]));
            i += end + 2;
            continue;
        }
        if let Some(r) = rest.strip_prefix("**")
            && let Some(end) = r.find("**").filter(|&n| n > 0)
        {
            out.push_str(&format!("<strong>{}</strong>", &r[..end]));
            i += end + 4;
            continue;
        }
        if let Some(r) = rest.strip_prefix('[')
            && let Some(mid) = r.find("](").filter(|&n| n > 0)
            && let Some(close) = r[mid + 2..].find(')')
            && is_web(&r[mid + 2..mid + 2 + close])
        {
            out.push_str(&format!("<a href=\"{}\">{}</a>", &r[mid + 2..mid + 2 + close], &r[..mid]));
            i += mid + close + 4;
            continue;
        }
        let at_word = e[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        if at_word && (rest.starts_with("https://") || rest.starts_with("http://")) {
            let word = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
            let url = word.trim_end_matches(['.', ',', ':', '!', '?', ')']);
            if is_web(url) {
                out.push_str(&format!("<a href=\"{url}\">{url}</a>"));
                i += url.len();
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        i += c.len_utf8().max(1);
    }
    out
}

/// The Markdown agents write, as HTML a mail client shows: paragraphs, line breaks, bullet lists and the
/// inline marks above. Text is escaped first, so no tag anyone typed survives.
pub fn markdown_html(src: &str) -> String {
    let mut blocks: Vec<Vec<&str>> = vec![vec![]];
    for line in src.lines() {
        if line.trim().is_empty() {
            if blocks.last().is_some_and(|b| !b.is_empty()) {
                blocks.push(vec![]);
            }
        } else if let Some(b) = blocks.last_mut() {
            b.push(line.trim_end());
        }
    }
    let mut out = String::new();
    for lines in blocks.iter().filter(|b| !b.is_empty()) {
        if lines.iter().all(|l| bullet(l).is_some()) {
            out.push_str("<ul style=\"margin:0 0 12px;padding-left:20px\">");
            for l in lines {
                out.push_str(&format!("<li>{}</li>", inline(&bullet(l).unwrap_or_default())));
            }
            out.push_str("</ul>");
        } else {
            out.push_str("<p style=\"margin:0 0 12px\">");
            out.push_str(&lines.iter().map(|l| inline(l)).collect::<Vec<_>>().join("<br>"));
            out.push_str("</p>");
        }
    }
    out
}

/// Where links in the emails point: the workspace's own address when the server is reached over HTTPS (or
/// does not say), else the server's own (`PUBLIC_URL`); none without either. The same rule the
/// platform's notification emails follow.
pub async fn link_base(k: &Kernel, tenant: Uuid) -> Option<String> {
    let public = k.settings.public_url.clone();
    if public.as_deref().is_none_or(|u| u.starts_with("https://"))
        && let Ok(Some(host)) = k.primary_host(tenant, "shell").await
    {
        return Some(format!("https://{host}"));
    }
    public.map(|u| u.trim_end_matches('/').to_string())
}

/// The ids in a message's `attachments` (`[{id, …}]` or `[id]`), each once.
pub fn file_ids(v: Option<&Value>) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = Vec::new();
    for x in v.and_then(Value::as_array).into_iter().flatten() {
        if let Some(id) = x.get("id").and_then(Value::as_str).or(x.as_str()).and_then(|s| s.parse().ok())
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

/// The files a new message names, as the stored files describe them (what the client wrote about a file's
/// name or size is not believed). Each must be a file this person uploaded and attached to nothing yet,
/// or one already on this ticket; at most [`MAX_FILES`] and [`MAX_BYTES`] together.
pub async fn check_files(hc: &mut HookCtx<'_>, ticket: Uuid, wanted: &Value) -> Result<Vec<Value>> {
    if !wanted.is_array() {
        return Err(Error::field("attachments", "must be a list of the files uploaded"));
    }
    let ids = file_ids(Some(wanted));
    if wanted.as_array().is_some_and(|a| a.len() != ids.len()) {
        return Err(Error::field("attachments", "each file is named by the id it was uploaded with, once"));
    }
    if ids.len() > MAX_FILES {
        return Err(Error::field("attachments", format!("at most {MAX_FILES} files in one message")));
    }
    if ids.is_empty() {
        return Ok(vec![]);
    }
    type Stored = (Uuid, String, String, i64, Option<String>, Option<Uuid>, Option<Uuid>);
    let rows: Vec<Stored> =
        sqlx::query_as("select id, file_name, content_type, size_bytes, entity, record_id, created_by from file_object where tenant_id = $1 and id = any($2)")
            .bind(hc.ctx.tenant)
            .bind(&ids)
            .fetch_all(&mut **hc.tx)
            .await?;
    let (mut out, mut total) = (Vec::new(), 0i64);
    for wanted in &ids {
        let usable = rows.iter().find(|r| {
            r.0 == *wanted
                && match (&r.4, r.5) {
                    (Some(e), Some(rec)) => e == "Ticket" && rec == ticket,
                    _ => hc.ctx.is_system || r.6 == Some(hc.ctx.user),
                }
        });
        let Some((id, name, ctype, size, ..)) = usable else {
            return Err(Error::field("attachments", format!("file {wanted} does not exist, is not yours, or belongs to something else")));
        };
        total += size;
        out.push(json!({"id": id, "file_name": name, "content_type": ctype, "size": size}));
    }
    if total > MAX_BYTES {
        return Err(Error::field("attachments", format!("files are limited to {} MB together", MAX_BYTES >> 20)));
    }
    Ok(out)
}

/// Puts the files that belonged to nothing yet on the ticket, so they list with it and follow its access.
pub async fn attach(tx: &mut Tx, tenant: Uuid, ids: &[Uuid], ticket: Uuid) -> Result<()> {
    if !ids.is_empty() {
        sqlx::query("update file_object set entity = 'Ticket', record_id = $3 where tenant_id = $1 and id = any($2) and entity is null and record_id is null")
            .bind(tenant)
            .bind(ids)
            .bind(ticket)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Renders the message template `helpdesk.reply` (the workspace's copy, else the package's) for a ticket and
/// the values it declares: the mail's subject and HTML.
async fn render_reply(k: &Kernel, sctx: &Ctx, ticket: &Doc, values: &Value) -> Result<(String, String)> {
    let def = k.message_template(sctx, "helpdesk.reply").await?;
    let languages = def["template"]["languages"].as_object().ok_or_else(|| Error::internal("the template helpdesk.reply has no languages"))?;
    let email = languages
        .get("en")
        .or_else(|| languages.values().next())
        .and_then(|l| l.get("email"))
        .ok_or_else(|| Error::internal("the template helpdesk.reply has no email"))?;
    let mut doc = ticket.clone();
    doc.remove("conversation");
    let vars = json!({"doc": doc, "values": values});
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|name| if name == "body" { minijinja::AutoEscape::Html } else { minijinja::AutoEscape::None });
    let bad = |e: minijinja::Error| Error::internal(format!("the template helpdesk.reply: {e}"));
    env.add_template_owned("subject", email["subject"].as_str().unwrap_or_default().to_string()).map_err(bad)?;
    env.add_template_owned("body", email["body"].as_str().unwrap_or_default().to_string()).map_err(bad)?;
    let subject: String =
        env.get_template("subject").and_then(|t| t.render(&vars)).map_err(bad)?.chars().filter(|c| !c.is_control()).take(MAX_SUBJECT).collect();
    let html = env.get_template("body").and_then(|t| t.render(&vars)).map_err(bad)?;
    Ok((subject, html))
}

/// The text of Markdown as plain lines, for the mail's plain part.
fn plain(markdown: &str) -> String {
    markdown.lines().map(|l| l.trim_end().replace("**", "").replace('`', "")).collect::<Vec<_>>().join("\n")
}

/// After a ticket is saved: puts the files its new messages carry on the ticket, and sends the replies that
/// were marked `emailed` when they were checked, to the ticket's contact (and its CC), through Mail when it is
/// installed. Done in the save's own transaction, so a reply that is refused sends nothing.
pub async fn dispatch(hc: &mut HookCtx<'_>, doc: &Doc, ticket: Uuid, fresh: &[Doc]) -> Result<()> {
    let files: Vec<Uuid> = fresh.iter().flat_map(|m| file_ids(m.get("attachments"))).collect();
    attach(hc.tx, hc.ctx.tenant, &files, ticket).await?;
    let Some(to) = s(doc, "contact_email").filter(|_| fresh.iter().any(|m| b(m, "emailed"))) else { return Ok(()) };
    let sctx = sys(hc.ctx);
    let number = s(doc, "number").unwrap_or("Ticket");
    let link = link_base(hc.kernel, hc.ctx.tenant).await.map(|base| format!("{base}/open/Ticket/{ticket}"));
    let via_mail = hc.ctx.schema.entity("Communication").is_some();
    let can_attach = via_mail || hc.kernel.mailer.as_ref().is_some_and(|m| m.can_attach());
    // Never to a support address itself: that would answer ourselves.
    let own: Vec<String> =
        hc.kernel.find_tx(&sctx, hc.tx, "SupportAddress", &[], None, 1000).await?.iter().filter_map(|a| s(a, "address").map(crate::inbound::norm)).collect();
    if own.contains(&crate::inbound::norm(to)) {
        return Ok(());
    }
    let copies: Vec<String> = crate::inbound::addresses(s(doc, "cc")).into_iter().filter(|a| !own.contains(a)).collect();
    // Threads onto the customer's latest message that came through Mail.
    let parent = rows(doc, "conversation")
        .iter()
        .rev()
        .find(|m| s(m, "author_kind") == Some("Customer") && s(m, "communication").is_some())
        .and_then(|m| id(m, "communication"));
    let account = match (via_mail, id(doc, "channel_address")) {
        (true, Some(a)) => {
            match hc.kernel.get_tx(&sctx, hc.tx, "SupportAddress", a, false).await.ok().and_then(|d| s(&d, "mail_account").map(str::to_string)) {
                Some(name) => hc
                    .kernel
                    .find_tx(&sctx, hc.tx, "EmailAccount", &[Filter::eq("account_name", name)], None, 1)
                    .await
                    .ok()
                    .and_then(|r| r.first().and_then(|d| id(d, "id"))),
                None => None,
            }
        }
        _ => None,
    };
    for m in fresh.iter().filter(|m| b(m, "emailed")) {
        let ids = file_ids(m.get("attachments"));
        let names: Vec<String> = m
            .get("attachments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| f.get("file_name").and_then(Value::as_str).map(str::to_string))
            .collect();
        let body = s(m, "body").unwrap_or_default();
        let signature = match s(m, "author") {
            Some(agent) => hc
                .kernel
                .find_tx(&sctx, hc.tx, "AgentProfile", &[Filter::eq("agent", agent.to_lowercase())], None, 1)
                .await?
                .first()
                .and_then(|p| s(p, "signature").map(markdown_html)),
            None => None,
        };
        let values =
            json!({"body": markdown_html(body), "signature": signature, "link": link, "files": names.join(", "), "attached": can_attach && !ids.is_empty()});
        // A template that is missing (a workspace not yet updated) or broken (edited) never stops a reply: the
        // agent's words go out in plain form, with the ticket's tag in the subject.
        let (title, html) = match render_reply(hc.kernel, &sctx, doc, &values).await {
            Ok(done) => done,
            Err(e) => {
                tracing::warn!(ticket = %ticket, "the reply template could not be used: {e}");
                let (token, subject_line) = (s(doc, "mail_token").unwrap_or_default(), s(doc, "subject").unwrap_or_default());
                (subject(number, token, subject_line), format!("{}{}", markdown_html(body), signature.clone().unwrap_or_default()))
            }
        };
        if via_mail {
            let mut c = Doc::new();
            for (f, v) in [
                ("channel", json!("Email")),
                ("direction", json!("Sent")),
                ("subject", json!(title)),
                ("content", json!(html)),
                ("text_content", json!(plain(body))),
                ("recipients", json!(to)),
                ("cc", if copies.is_empty() { Value::Null } else { json!(copies.join(", ")) }),
                ("reference_entity", json!("Ticket")),
                ("reference_id", json!(ticket.to_string())),
                ("reply_to", json!(parent.map(|p| p.to_string()))),
                ("email_account", json!(account.map(|a| a.to_string()))),
                ("attachments", m.get("attachments").cloned().unwrap_or_else(|| json!([]))),
                ("headers", json!({"X-Ferrum-Helpdesk": number})),
                ("seen", json!(true)),
            ] {
                c.insert(f.into(), v);
            }
            let sent = hc.kernel.insert_tx(&sctx, hc.tx, "Communication", c).await?;
            if let Some(message) = id(m, "id") {
                sqlx::query("update e_ticket__conversation set message_id = $1, communication = $2 where tenant_id = $3 and id = $4")
                    .bind(s(&sent, "message_id"))
                    .bind(s(&sent, "id"))
                    .bind(hc.ctx.tenant)
                    .bind(message)
                    .execute(&mut **hc.tx)
                    .await?;
            }
        } else {
            // A mailer that cannot attach (the development stub) sends the words and points to the ticket.
            for address in std::iter::once(to).chain(copies.iter().map(String::as_str)) {
                if can_attach && !ids.is_empty() {
                    hc.kernel.queue_mail_with(hc.tx, hc.ctx.tenant, address, &title, &html, &ids).await?;
                } else {
                    hc.kernel.queue_mail(hc.tx, hc.ctx.tenant, address, &title, &html).await?;
                }
            }
        }
    }
    Ok(())
}

/// Whether a new public reply goes out by email: the ticket has a contact address, the server has a
/// mailer (or Mail is installed), the platform's mail accepts the address, and nothing restricts writing to it.
/// If not, the reply is still in the conversation, and in the portal.
pub async fn goes_by_email(hc: &mut HookCtx<'_>, existing: bool, contact: Option<&str>, number: &str, title: &str) -> Result<bool> {
    let Some(to) = contact.filter(|_| existing && (hc.kernel.mailer.is_some() || hc.ctx.schema.entity("Communication").is_some())) else { return Ok(false) };
    if ferrum_kernel::outbox::check_mail(to, &subject(number, "x", title)).is_err() {
        return Ok(false);
    }
    hc.kernel.may_contact(hc.tx, hc.ctx.tenant, to).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_names_the_ticket_and_fits_the_platforms_limit() {
        assert_eq!(subject("TKT-2026-000123", "k3j9x2m1", "Printer jam"), "Re: [TKT-2026-000123/k3j9x2m1] Printer jam");
        let long = subject("TKT-2026-000123", "k3j9x2m1", &"é".repeat(400));
        assert!(long.len() <= 300 && long.starts_with("Re: [TKT-2026-000123/k3j9x2m1] é"), "{}", long.len());
        assert!(ferrum_kernel::outbox::check_mail("ada@shop.test", &long).is_ok());
        assert_eq!(subject("T-1", "abcdefgh", "two\nlines"), "Re: [T-1/abcdefgh] twolines");
        assert!(crate::inbound::tag(&subject("T-1", "abcdefgh", "x")).is_some(), "what goes out is what comes back");
    }

    #[test]
    fn markdown_becomes_paragraphs_lists_and_the_inline_marks_and_nothing_else() {
        assert_eq!(
            markdown_html("Hello **Ada**,\nthanks.\n\n\n- one\n- `two`\n\nSee [the guide](https://help.test/a?x=1&y=2)."),
            concat!(
                "<p style=\"margin:0 0 12px\">Hello <strong>Ada</strong>,<br>thanks.</p>",
                "<ul style=\"margin:0 0 12px;padding-left:20px\"><li>one</li><li><code>two</code></li></ul>",
                "<p style=\"margin:0 0 12px\">See <a href=\"https://help.test/a?x=1&amp;y=2\">the guide</a>.</p>"
            )
        );
    }

    #[test]
    fn a_tag_or_a_script_link_a_person_typed_is_text() {
        let html = markdown_html("<script>alert(1)</script> [x](javascript:alert(1)) [y](https://a.test/\"onmouseover=\"x)");
        assert!(!html.contains("<script") && !html.contains("href=\"javascript"), "{html}");
        assert!(html.contains("&lt;script&gt;") && html.contains("[x](javascript:alert(1))"), "{html}");
        assert!(!html.contains("onmouseover=\""), "{html}");
    }

    #[test]
    fn bare_addresses_become_links_without_their_closing_punctuation() {
        assert_eq!(inline("Go to https://help.test/reset."), "Go to <a href=\"https://help.test/reset\">https://help.test/reset</a>.");
        assert_eq!(inline("not-a-link xhttps://nope"), "not-a-link xhttps://nope");
        assert_eq!(inline("2 * 3 * 4 and a_b_c"), "2 * 3 * 4 and a_b_c");
    }

    #[test]
    fn files_are_named_by_id_once() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        assert_eq!(file_ids(Some(&json!([{"id": a.to_string(), "file_name": "x"}, b.to_string(), {"id": a.to_string()}, {"id": "nonsense"}]))), vec![a, b]);
        assert!(file_ids(None).is_empty() && file_ids(Some(&json!("x"))).is_empty());
    }
}
